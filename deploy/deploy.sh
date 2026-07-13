#!/bin/sh
set -eu

APP_NAME='ronitnath'
SERVICE_USER='ronitnath-app'
PROFILE="/nix/var/nix/profiles/$APP_NAME"
SITE_UNIT="$APP_NAME-site.service"
ADMIN_UNIT="$APP_NAME-admin.service"
DATA_DIR="/data/apps/$APP_NAME"
PHOTO_DIR="$DATA_DIR/photos"
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(dirname -- "$SCRIPT_DIR")
SITE_UNIT_SOURCE="$SCRIPT_DIR/$SITE_UNIT"
ADMIN_UNIT_SOURCE="$SCRIPT_DIR/$ADMIN_UNIT"

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
    # Site owns migrations. Start it first; admin's Restart=always handles the
    # narrow interval before a newly added migration has completed.
    sudo systemctl restart "$SITE_UNIT"
    sudo systemctl restart "$ADMIN_UNIT"
}

deploy() {
    require_command nix
    NIX_ENV=$(resolve_command nix-env)
    require_command sudo
    require_command systemctl
    [ -f "$PROJECT_ROOT/flake.nix" ] || fail "flake.nix not found at $PROJECT_ROOT"

    (
        cd "$PROJECT_ROOT"
        nix build .#default
        [ -e ./result ] || [ -L ./result ] || fail "nix build did not create ./result"
        sudo "$NIX_ENV" --profile "$PROFILE" --set ./result
    )
    restart_services
}

rollback() {
    NIX_ENV=$(resolve_command nix-env)
    require_command sudo
    require_command systemctl
    [ -e "$PROFILE" ] || [ -L "$PROFILE" ] || fail "profile does not exist; deploy once before rollback"

    sudo "$NIX_ENV" --profile "$PROFILE" --rollback
    restart_services
}

status() {
    NIX_ENV=$(resolve_command nix-env)
    require_command sudo
    require_command systemctl

    sudo systemctl status "$SITE_UNIT" "$ADMIN_UNIT" --no-pager || true
    [ -e "$PROFILE" ] || [ -L "$PROFILE" ] || fail "profile does not exist; deploy once before status"
    printf '\nCurrent Nix profile generation:\n'
    sudo "$NIX_ENV" --profile "$PROFILE" --list-generations | awk '/\(current\)/'
}

init() {
    require_command id
    require_command install
    require_command sudo
    require_command systemctl
    USERADD=$(resolve_command useradd)
    [ -f "$SITE_UNIT_SOURCE" ] || fail "unit file not found: $SITE_UNIT_SOURCE"
    [ -f "$ADMIN_UNIT_SOURCE" ] || fail "unit file not found: $ADMIN_UNIT_SOURCE"

    if ! id "$SERVICE_USER" >/dev/null 2>&1; then
        sudo "$USERADD" --system --user-group --home-dir "$DATA_DIR" \
            --shell /usr/sbin/nologin "$SERVICE_USER"
    fi
    APP_GROUP=$(id -gn "$SERVICE_USER")
    sudo install -d -m 0750 -o "$SERVICE_USER" -g "$APP_GROUP" \
        "$DATA_DIR" "$PHOTO_DIR"
    sudo install -m 0644 "$SITE_UNIT_SOURCE" "/etc/systemd/system/$SITE_UNIT"
    sudo install -m 0644 "$ADMIN_UNIT_SOURCE" "/etc/systemd/system/$ADMIN_UNIT"
    sudo systemctl daemon-reload
    sudo systemctl enable "$SITE_UNIT" "$ADMIN_UNIT"
}

[ "$#" -le 1 ] || fail "usage: $0 [deploy|rollback|status|init]"
case "${1:-deploy}" in
    deploy) deploy ;;
    rollback) rollback ;;
    status) status ;;
    init) init ;;
    *) fail "unknown subcommand '$1'; expected deploy, rollback, status, or init" ;;
esac
