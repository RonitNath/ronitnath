#!/bin/sh
set -eu

APP_NAME='ronitnath'
SERVICE_USER='ronitnath-app'
SITE_UNIT="$APP_NAME-site.service"
ADMIN_UNIT="$APP_NAME-admin.service"
DATA_DIR="/data/apps/$APP_NAME"
PHOTO_DIR="$DATA_DIR/photos"
RELEASES_DIR="$DATA_DIR/releases"
CURRENT_LINK="$DATA_DIR/current"
PREVIOUS_LINK="$DATA_DIR/previous"
KEEP_RELEASES=5
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
    dirty_json=false
    dirty_suffix=''
    if [ -n "$(git -C "$PROJECT_ROOT" status --porcelain)" ]; then
        dirty_json=true
        dirty_suffix='-dirty'
        printf '%s\n' \
            "deploy: WARNING: working tree is dirty; this release is not tied to a clean revision" >&2
    fi
    RELEASE_DIR="$RELEASES_DIR/$(date -u +%Y%m%dT%H%M%SZ)-$revision$dirty_suffix"

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

    manifest=$(mktemp)
    printf '{\n  "app": "%s",\n  "revision": "%s",\n  "dirty": %s,\n  "site_sha256": "%s",\n  "admin_sha256": "%s",\n  "cargo_lock_sha256": "%s"\n}\n' \
        "$APP_NAME" "$revision" "$dirty_json" \
        "$(sha256sum "$site_binary" | cut -d' ' -f1)" \
        "$(sha256sum "$admin_binary" | cut -d' ' -f1)" \
        "$(sha256sum "$PROJECT_ROOT/Cargo.lock" | cut -d' ' -f1)" >"$manifest"
    sudo install -m 0644 "$manifest" "$RELEASE_DIR/release.json"
    rm -f "$manifest"
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
    build
    stamp_release
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
