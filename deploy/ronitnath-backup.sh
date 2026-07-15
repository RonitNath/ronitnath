#!/bin/sh
set -eu

umask 077

APP_NAME='ronitnath'
SERVICE_USER='ronitnath-app'
DATA_DIR="/data/apps/$APP_NAME"
BACKUPS_DIR="$DATA_DIR/backups"
LIVE_DB="$DATA_DIR/app.db"
RESTIC_ENV_FILE=/etc/restic/env
STAGING_DIR="/run/$APP_NAME-backup-staging"
STAGED_DB="$STAGING_DIR/app.db"
SCRATCH_DIR=''
OPERATION=''

fail() {
    printf '%s\n' "backup: $*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

load_restic_environment() {
    [ -f "$RESTIC_ENV_FILE" ] \
        || fail "$RESTIC_ENV_FILE is missing; provision the root-only restic host configuration"
    # This root-owned host file is never carried by the application repository.
    # shellcheck disable=SC1090
    . "$RESTIC_ENV_FILE"
    [ -n "${RESTIC_REPOSITORY:-}" ] || fail "$RESTIC_ENV_FILE does not define RESTIC_REPOSITORY"
    [ -n "${AWS_ACCESS_KEY_ID:-}" ] || fail "$RESTIC_ENV_FILE does not define AWS_ACCESS_KEY_ID"
    [ -n "${AWS_SECRET_ACCESS_KEY:-}" ] \
        || fail "$RESTIC_ENV_FILE does not define AWS_SECRET_ACCESS_KEY"
    [ -n "${RESTIC_PASSWORD:-}" ] || fail "$RESTIC_ENV_FILE does not define RESTIC_PASSWORD"
    export RESTIC_REPOSITORY RESTIC_PASSWORD
    export AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY
}

metrics_directory() {
    if [ -d /var/lib/prometheus/node-exporter ]; then
        printf '%s\n' /var/lib/prometheus/node-exporter
    elif [ -d /var/lib/node_exporter/textfile_collector ]; then
        printf '%s\n' /var/lib/node_exporter/textfile_collector
    fi
}

write_metrics() {
    operation=$1
    exit_code=$2
    metrics_dir=$(metrics_directory)
    [ -n "$metrics_dir" ] || return 0

    metric_app=$(printf '%s' "$APP_NAME" | tr '-' '_')
    metric_prefix="${metric_app}_${operation}"
    metric_file="$metrics_dir/${APP_NAME}_${operation}.prom"
    last_success=0
    if [ -f "$metric_file" ]; then
        previous_success=$(awk -v metric="${metric_prefix}_last_success_timestamp_seconds" \
            '$1 == metric { print $2; exit }' "$metric_file")
        case "$previous_success" in
            ''|*[!0-9]*) ;;
            *) last_success=$previous_success ;;
        esac
    fi
    if [ "$exit_code" -eq 0 ]; then
        last_success=$(date +%s)
    fi

    metric_tmp=$(mktemp "$metrics_dir/.${APP_NAME}_${operation}.prom.XXXXXX") || return 1
    if ! printf '# TYPE %s_last_success_timestamp_seconds gauge\n%s_last_success_timestamp_seconds %s\n# TYPE %s_exit_code gauge\n%s_exit_code %s\n' \
        "$metric_prefix" "$metric_prefix" "$last_success" \
        "$metric_prefix" "$metric_prefix" "$exit_code" >"$metric_tmp"; then
        rm -f "$metric_tmp"
        return 1
    fi
    chmod 0644 "$metric_tmp" || {
        rm -f "$metric_tmp"
        return 1
    }
    mv -f "$metric_tmp" "$metric_file"
}

on_exit() {
    exit_code=$1
    trap - 0 HUP INT TERM
    set +e

    cleanup_code=0
    if [ "$OPERATION" = backup ]; then
        rm -rf "$STAGING_DIR" || cleanup_code=$?
    fi
    if [ -n "$SCRATCH_DIR" ]; then
        rm -rf "$SCRATCH_DIR" || cleanup_code=$?
    fi
    if [ "$exit_code" -eq 0 ] && [ "$cleanup_code" -ne 0 ]; then
        printf '%s\n' "backup: could not clean temporary backup data" >&2
        exit_code=$cleanup_code
    fi
    if ! write_metrics "$OPERATION" "$exit_code"; then
        printf '%s\n' "backup: could not atomically publish $OPERATION metrics" >&2
        [ "$exit_code" -ne 0 ] || exit_code=1
    fi
    exit "$exit_code"
}

backup() {
    load_restic_environment
    require_command restic
    require_command install
    require_command sudo
    require_command id

    rm -rf "$STAGING_DIR"
    app_group=$(id -gn "$SERVICE_USER") \
        || fail "could not resolve the service group for $SERVICE_USER"
    install -d -m 0700 -o "$SERVICE_USER" -g "$app_group" "$STAGING_DIR"
    set -- "$DATA_DIR"
    if [ -e "$LIVE_DB" ]; then
        require_command sqlite3
        SQLITE3=$(command -v sqlite3)
        sudo -u "$SERVICE_USER" "$SQLITE3" -batch -bail "$LIVE_DB" \
            "VACUUM INTO '$STAGED_DB';" \
            || fail "VACUUM INTO failed while staging $LIVE_DB"
        integrity_output=$(sudo -u "$SERVICE_USER" "$SQLITE3" -batch -bail -noheader \
            "$STAGED_DB" 'PRAGMA integrity_check;') \
            || fail "integrity_check failed on the staged database"
        [ "$integrity_output" = ok ] \
            || fail "staged database integrity_check did not return exactly 'ok'"
        set -- "$STAGED_DB" "$DATA_DIR"
    fi

    restic backup --tag "$APP_NAME" \
        --exclude "$LIVE_DB" \
        --exclude "$LIVE_DB-wal" \
        --exclude "$LIVE_DB-shm" \
        --exclude "$BACKUPS_DIR" \
        "$@"
    # Repository-wide prune and check are host-level jobs, not per-app work.
    restic forget --tag "$APP_NAME" \
        --keep-daily 7 --keep-weekly 4 --keep-monthly 6
}

drill() {
    load_restic_environment
    require_command restic
    require_command sqlite3
    require_command mktemp

    SCRATCH_DIR=$(mktemp -d "/run/${APP_NAME}-restore-drill.XXXXXX")
    restic restore latest --tag "$APP_NAME" --target "$SCRATCH_DIR"
    restored_db="$SCRATCH_DIR$STAGED_DB"
    [ -f "$restored_db" ] \
        || fail "latest tagged snapshot does not contain the staged database copy"

    integrity_output=$(sqlite3 -batch -bail -noheader "$restored_db" \
        'PRAGMA integrity_check;') \
        || fail "integrity_check failed on the restored database"
    [ "$integrity_output" = ok ] \
        || fail "restored database integrity_check did not return exactly 'ok'"
    table_count=$(sqlite3 -batch -bail -noheader "$restored_db" \
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%';") \
        || fail "could not inspect user tables in the restored database"
    case "$table_count" in
        ''|*[!0-9]*) fail "restored database returned an invalid user-table count" ;;
    esac
    [ "$table_count" -gt 0 ] \
        || fail "restored database contains no user tables"
}

[ "$#" -eq 1 ] || fail "usage: $0 <backup|drill>"
case "$1" in
    backup) OPERATION=backup ;;
    drill) OPERATION=restore_drill ;;
    *) fail "unknown subcommand '$1'; expected backup or drill" ;;
esac

trap 'on_exit $?' 0
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

case "$1" in
    backup) backup ;;
    drill) drill ;;
esac
