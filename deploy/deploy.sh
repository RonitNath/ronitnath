#!/bin/sh
set -eu

APP_NAME='ronitnath'
SERVICE_USER='ronitnath-app'
SITE_UNIT="$APP_NAME-site.service"
ADMIN_UNIT="$APP_NAME-admin.service"
SITE_SOCKET="$APP_NAME-site.socket"
ADMIN_SOCKET="$APP_NAME-admin.socket"
DATA_DIR="/data/apps/$APP_NAME"
PHOTO_DIR="$DATA_DIR/photos"
RELEASES_DIR="$DATA_DIR/releases"
CURRENT_LINK="$DATA_DIR/current"
PREVIOUS_LINK="$DATA_DIR/previous"
BACKUPS_DIR="$DATA_DIR/backups"
LIVE_DB="$DATA_DIR/app.db"
KEEP_RELEASES=5
KEEP_BACKUPS=10
PRE_MIGRATION_SNAPSHOT=''
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(dirname -- "$SCRIPT_DIR")
SITE_UNIT_SOURCE="$SCRIPT_DIR/$SITE_UNIT"
ADMIN_UNIT_SOURCE="$SCRIPT_DIR/$ADMIN_UNIT"
SITE_SOCKET_SOURCE="$SCRIPT_DIR/$SITE_SOCKET"
ADMIN_SOCKET_SOURCE="$SCRIPT_DIR/$ADMIN_SOCKET"

fail() {
    printf '%s\n' "deploy: $*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

# sudo uses a clean secure_path on the target host. Resolve privileged helpers
# first, with the multi-user Nix and system administration paths explicitly in
# scope, then pass their absolute paths through sudo.
resolve_command() {
    resolved=$(
        PATH="$PATH:/usr/sbin:/sbin:/nix/var/nix/profiles/default/bin" \
            command -v "$1"
    ) || fail "required command not found: $1"
    [ -n "$resolved" ] || fail "required command not found: $1"
    printf '%s\n' "$resolved"
}

restart_services() {
    # The socket units keep both listen sockets bound while the services
    # restart, so connections queue in the kernel instead of being refused;
    # the binaries drain in-flight requests on SIGTERM.
    # Site owns migrations. Start it first; admin's Restart=always handles the
    # narrow interval before a newly added migration has completed.
    sudo systemctl restart "$SITE_UNIT"
    sudo systemctl restart "$ADMIN_UNIT"
    # Readiness gate: Restart=always masks an immediate crash, so require both
    # units to stay active before declaring the deploy done.
    checks=0
    while [ "$checks" -lt 5 ]; do
        sleep 1
        sudo systemctl is-active --quiet "$SITE_UNIT" "$ADMIN_UNIT" \
            || fail "a unit is not active after restart; inspect journalctl and consider '$0 rollback'"
        checks=$((checks + 1))
    done
}

# Incremental host build: the flake devshell pins the toolchain while the
# checkout's target/ directory carries the compilation cache, so only crates
# affected by a change recompile. ts/build.sh rebundles with esbuild in
# milliseconds.
build() {
    require_command nix
    require_command git
    [ -f "$PROJECT_ROOT/flake.nix" ] || fail "flake.nix not found at $PROJECT_ROOT"
    cd "$PROJECT_ROOT"
    nix develop --command sh -c '
        set -eu
        export SQLX_OFFLINE=true
        cargo build --release --locked --bin site --bin admin
        sh ts/build.sh
    '
}

stamp_release() {
    revision=$(git -C "$PROJECT_ROOT" rev-parse --short=12 HEAD)
    release_stamp=$(date -u +%Y%m%dT%H%M%SZ)
    dirty_json=false
    dirty_suffix=''
    if [ -n "$(git -C "$PROJECT_ROOT" status --porcelain)" ]; then
        dirty_json=true
        dirty_suffix='-dirty'
        printf '%s\n' \
            "deploy: WARNING: working tree is dirty; this release is not tied to a clean revision" >&2
    fi
    RELEASE_DIR="$RELEASES_DIR/$release_stamp-$revision$dirty_suffix"

    site_binary="$PROJECT_ROOT/target/release/site"
    admin_binary="$PROJECT_ROOT/target/release/admin"
    [ -x "$site_binary" ] || fail "built binary not found: $site_binary"
    [ -x "$admin_binary" ] || fail "built binary not found: $admin_binary"
    [ -f "$PROJECT_ROOT/static/dist/site.js" ] || fail "frontend bundle missing: static/dist/site.js"
    # Releases are root-owned and read-only to the service account; only the
    # symlink flip below changes what the units run.
    sudo install -d -m 0755 "$RELEASES_DIR" "$RELEASE_DIR" "$RELEASE_DIR/bin"
    sudo install -m 0755 "$site_binary" "$RELEASE_DIR/bin/site"
    sudo install -m 0755 "$admin_binary" "$RELEASE_DIR/bin/admin"
    sudo cp -R "$PROJECT_ROOT/static" "$RELEASE_DIR/static"
    sudo chmod -R a+rX "$RELEASE_DIR/static"
}

write_manifest() {
    site_binary="$PROJECT_ROOT/target/release/site"
    admin_binary="$PROJECT_ROOT/target/release/admin"
    manifest=$(mktemp)
    printf '{\n  "app": "%s",\n  "revision": "%s",\n  "dirty": %s,\n  "site_sha256": "%s",\n  "admin_sha256": "%s",\n  "cargo_lock_sha256": "%s",\n  "pre_migration_snapshot": "%s"\n}\n' \
        "$APP_NAME" "$revision" "$dirty_json" \
        "$(sha256sum "$site_binary" | cut -d' ' -f1)" \
        "$(sha256sum "$admin_binary" | cut -d' ' -f1)" \
        "$(sha256sum "$PROJECT_ROOT/Cargo.lock" | cut -d' ' -f1)" \
        "$PRE_MIGRATION_SNAPSHOT" >"$manifest"
    sudo install -m 0644 "$manifest" "$RELEASE_DIR/release.json"
    rm -f "$manifest"
}

normalize_migration_version() {
    migration_version=$1
    while [ "${migration_version#0}" != "$migration_version" ]; do
        migration_version=${migration_version#0}
    done
    [ -n "$migration_version" ] || migration_version=0
    printf '%s\n' "$migration_version"
}

version_in_list() {
    version_list=$1
    version_wanted=$2
    for listed_version in $version_list; do
        [ "$listed_version" = "$version_wanted" ] && return 0
    done
    return 1
}

prune_backups() {
    backup_paths=$(sudo find "$BACKUPS_DIR" -maxdepth 1 -type f -name '*-pre-*.db' -printf '%p\n') \
        || fail "could not enumerate pre-migration snapshots in $BACKUPS_DIR"
    sorted_backup_paths=$(printf '%s\n' "$backup_paths" | sort -r) \
        || fail "could not sort pre-migration snapshots in $BACKUPS_DIR"
    old_ifs=$IFS
    IFS='
'
    kept=0
    for backup_path in $sorted_backup_paths; do
        kept=$((kept + 1))
        [ "$kept" -le "$KEEP_BACKUPS" ] && continue
        sudo rm -f "$backup_path" \
            || fail "could not prune old pre-migration snapshot $backup_path"
    done
    IFS=$old_ifs
}

state_preflight() {
    if ! sudo test -e "$LIVE_DB"; then
        printf '%s\n' "deploy: live database does not exist; skipping pre-migration snapshot"
        return
    fi

    SQLITE3=$(PATH="$PATH:/usr/sbin:/sbin:/nix/var/nix/profiles/default/bin" command -v sqlite3) \
        || fail "sqlite3 CLI is required to inspect the existing database; install sqlite3 and retry"

    release_versions=''
    for migration in "$PROJECT_ROOT"/migrations/*.sql; do
        [ -f "$migration" ] || continue
        migration_name=${migration##*/}
        version_prefix=${migration_name%%_*}
        case "$version_prefix" in
            ''|*[!0-9]*)
                fail "migration filename does not start with an integer version: $migration_name"
                ;;
        esac
        normalized_version=$(normalize_migration_version "$version_prefix")
        if ! version_in_list "$release_versions" "$normalized_version"; then
            release_versions="$release_versions $normalized_version"
        fi
    done

    table_exists=$(sudo -u "$SERVICE_USER" "$SQLITE3" -batch -bail -noheader "$LIVE_DB" \
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations';") \
        || fail "could not inspect _sqlx_migrations in $LIVE_DB"

    needs_snapshot=0
    divergent_versions=''
    if [ "$table_exists" != 1 ]; then
        printf '%s\n' \
            "deploy: _sqlx_migrations is missing; treating all release migrations as pending"
        needs_snapshot=1
    else
        database_versions=$(sudo -u "$SERVICE_USER" "$SQLITE3" -batch -bail -noheader "$LIVE_DB" \
            "SELECT version FROM _sqlx_migrations ORDER BY version;") \
            || fail "could not read migration versions from _sqlx_migrations in $LIVE_DB"
        for database_version in $database_versions; do
            normalized_version=$(normalize_migration_version "$database_version")
            if ! version_in_list "$release_versions" "$normalized_version"; then
                divergent_versions="$divergent_versions $normalized_version"
            fi
        done
        for release_version in $release_versions; do
            if ! version_in_list "$database_versions" "$release_version"; then
                needs_snapshot=1
            fi
        done
    fi

    if [ -n "$divergent_versions" ]; then
        if [ "${DEPLOY_ALLOW_DIVERGENT:-0}" != 1 ]; then
            fail "database has migration versions absent from this release:$divergent_versions; refusing a potentially older schema (set DEPLOY_ALLOW_DIVERGENT=1 to override with a mandatory snapshot)"
        fi
        printf '%s\n' \
            "deploy: WARNING: DEPLOY_ALLOW_DIVERGENT=1 permits divergent migration versions:$divergent_versions"
        needs_snapshot=1
    fi

    if [ "$needs_snapshot" -eq 0 ]; then
        printf '%s\n' "deploy: no pending migrations; skipping snapshot"
        return
    fi

    app_group=$(id -gn "$SERVICE_USER") \
        || fail "could not resolve the service group for $SERVICE_USER"
    sudo install -d -m 0750 -o "$SERVICE_USER" -g "$app_group" "$BACKUPS_DIR" \
        || fail "could not create pre-migration snapshot directory $BACKUPS_DIR"
    PRE_MIGRATION_SNAPSHOT="$BACKUPS_DIR/$release_stamp-pre-$revision.db"
    sudo test ! -e "$PRE_MIGRATION_SNAPSHOT" \
        || fail "pre-migration snapshot already exists: $PRE_MIGRATION_SNAPSHOT"
    sudo -u "$SERVICE_USER" "$SQLITE3" -batch -bail "$LIVE_DB" \
        "VACUUM INTO '$PRE_MIGRATION_SNAPSHOT';" \
        || fail "pre-migration VACUUM INTO snapshot failed; current release remains active"
    integrity_output=$(sudo -u "$SERVICE_USER" "$SQLITE3" -batch -bail -noheader \
        "$PRE_MIGRATION_SNAPSHOT" "PRAGMA integrity_check;") \
        || fail "pre-migration snapshot integrity_check failed; current release remains active"
    [ "$integrity_output" = ok ] \
        || fail "pre-migration snapshot integrity_check did not return exactly 'ok'; current release remains active"
    sudo chmod 0640 "$PRE_MIGRATION_SNAPSHOT" \
        || fail "could not secure pre-migration snapshot permissions"
    prune_backups
    printf '%s\n' "deploy: pre-migration snapshot verified: $PRE_MIGRATION_SNAPSHOT"
}

# Replacing a symlink by rename is the atomic step: the units resolve either
# the old release or the new one, never a partial state.
point_link() {
    staged="$2.next.$$"
    sudo ln -s "$1" "$staged"
    sudo mv -T "$staged" "$2"
}

# The data root is only traversable by the service account and root, so every
# read of the release pointers goes through sudo like the writes do.
link_target() {
    sudo readlink "$1" 2>/dev/null || true
}

switch_current() {
    old_target=$(link_target "$CURRENT_LINK")
    point_link "$RELEASE_DIR" "$CURRENT_LINK"
    if [ -n "$old_target" ] && [ "$old_target" != "$RELEASE_DIR" ]; then
        point_link "$old_target" "$PREVIOUS_LINK"
    fi
}

prune_releases() {
    keep_current=$(link_target "$CURRENT_LINK")
    keep_previous=$(link_target "$PREVIOUS_LINK")
    kept=0
    for release in $(sudo ls -1 "$RELEASES_DIR" | sort -r); do
        path="$RELEASES_DIR/$release"
        if [ "$path" = "$keep_current" ] || [ "$path" = "$keep_previous" ] \
            || [ "$kept" -lt "$KEEP_RELEASES" ]; then
            kept=$((kept + 1))
            continue
        fi
        sudo rm -rf "$path"
    done
}

deploy() {
    require_command sudo
    require_command systemctl
    require_command sha256sum
    require_command id
    build
    stamp_release
    state_preflight
    write_manifest
    switch_current
    restart_services
    prune_releases
}

rollback() {
    require_command sudo
    require_command systemctl
    current_target=$(link_target "$CURRENT_LINK")
    previous_target=$(link_target "$PREVIOUS_LINK")
    [ -n "$current_target" ] || fail "current release pointer missing; deploy once before rollback"
    [ -n "$previous_target" ] || fail "no previous release recorded; nothing to roll back to"
    # Binary rollback leaves the schema at its newer N-1-compatible shape;
    # restoring a pre-migration snapshot is a deliberate manual operation.
    point_link "$previous_target" "$CURRENT_LINK"
    point_link "$current_target" "$PREVIOUS_LINK"
    restart_services
}

status() {
    require_command sudo
    require_command systemctl
    sudo systemctl status "$SITE_UNIT" "$ADMIN_UNIT" --no-pager || true
    current_target=$(link_target "$CURRENT_LINK")
    [ -n "$current_target" ] || fail "current release pointer missing; deploy once before status"
    printf '\nCurrent release: %s\n' "$current_target"
    sudo cat "$current_target/release.json" 2>/dev/null || true
    previous_target=$(link_target "$PREVIOUS_LINK")
    [ -z "$previous_target" ] \
        || printf 'Previous release: %s\n' "$previous_target"
}

init() {
    require_command id
    require_command install
    require_command sudo
    require_command systemctl
    USERADD=$(resolve_command useradd)
    [ -f "$SITE_UNIT_SOURCE" ] || fail "unit file not found: $SITE_UNIT_SOURCE"
    [ -f "$ADMIN_UNIT_SOURCE" ] || fail "unit file not found: $ADMIN_UNIT_SOURCE"
    [ -f "$SITE_SOCKET_SOURCE" ] || fail "socket unit file not found: $SITE_SOCKET_SOURCE"
    [ -f "$ADMIN_SOCKET_SOURCE" ] || fail "socket unit file not found: $ADMIN_SOCKET_SOURCE"

    if ! id "$SERVICE_USER" >/dev/null 2>&1; then
        sudo "$USERADD" --system --user-group --home-dir "$DATA_DIR" \
            --shell /usr/sbin/nologin "$SERVICE_USER"
    fi
    APP_GROUP=$(id -gn "$SERVICE_USER")
    sudo install -d -m 0750 -o "$SERVICE_USER" -g "$APP_GROUP" \
        "$DATA_DIR" "$PHOTO_DIR"
    sudo install -d -m 0755 "$RELEASES_DIR"
    sudo install -m 0644 "$SITE_UNIT_SOURCE" "/etc/systemd/system/$SITE_UNIT"
    sudo install -m 0644 "$ADMIN_UNIT_SOURCE" "/etc/systemd/system/$ADMIN_UNIT"
    sudo install -m 0644 "$SITE_SOCKET_SOURCE" "/etc/systemd/system/$SITE_SOCKET"
    sudo install -m 0644 "$ADMIN_SOCKET_SOURCE" "/etc/systemd/system/$ADMIN_SOCKET"
    sudo systemctl daemon-reload
    sudo systemctl enable "$SITE_UNIT" "$ADMIN_UNIT" "$SITE_SOCKET" "$ADMIN_SOCKET"
}

[ "$#" -le 1 ] || fail "usage: $0 [deploy|rollback|status|init]"
case "${1:-deploy}" in
    deploy) deploy ;;
    rollback) rollback ;;
    status) status ;;
    init) init ;;
    *) fail "unknown subcommand '$1'; expected deploy, rollback, status, or init" ;;
esac
