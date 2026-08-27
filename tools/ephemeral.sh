#!/usr/bin/env bash
# One ephemeral rn-site instance per worktree — story G20.
#
#   tools/ephemeral.sh up        build if needed, start, wait for /readyz
#   tools/ephemeral.sh status    what this worktree's instance reports
#   tools/ephemeral.sh stop      SIGTERM it, wait, drop the pid file
#   tools/ephemeral.sh reset     stop, then wipe the state directory
#   tools/ephemeral.sh backup [dir]   stop, back up, leave it stopped
#   tools/ephemeral.sh restore <dir>  stop, wipe, put it back
#   tools/ephemeral.sh admin ...      `rn-site admin` against this instance
#
# Everything that could collide between two worktrees on one machine is
# derived from the worktree's own absolute path: the three ports, the state
# directory, the id key and the signing key. Two checkouts of this repo can
# therefore be up at the same time and neither knows about the other — which
# is the whole point, since a second agent working a second branch needs a
# running deployment of its own, not a turn at :3004.
#
#   slot   = sha256(worktree path) mod 100
#   app    = 3300 + slot   the HTTP surface, and the public origin
#   admin  = 3400 + slot   the loopback-only /admin/* listener (G21.2)
#   raft   = 8300 + slot   hiqlite's peer protocol (single voter, dev topology)
#   api    = 8400 + slot   hiqlite's own API listener
#
# `cargo run` on :3004 and `tools/cluster.sh` on 3161-3163 are outside every
# one of those ranges, so an ephemeral instance never fights them either.
#
# State lives under `<worktree>/target/ephemeral/`, which is gitignored: the
# database, the log, the pid file and the two keys. Nothing here is committed
# and nothing here survives `reset`.
#
# The keys are minted once per worktree and kept, rather than minted per boot,
# because both of them name things that outlive a restart: the id key derives
# every public id, and the signing key is what the OpenID Provider's stored
# keys are sealed under. A dev process that minted fresh ones would rename
# every object and orphan every signing key on every start.
#
# `backup` and `restore` wrap the two subcommands (F16.4), and both stop the
# instance first: they open the data directory directly, and hiqlite holds an
# exclusive lock on it — which the subcommand refuses on, naming the pid.
#
# A backup directory holds the two key files as well as the rows, under
# `keys/`, mode 0600. That is not the manifest carrying a secret — the manifest
# carries a *fingerprint* and never the key — it is this wrapper keeping the
# one thing a `reset` would otherwise destroy. Public ids are derived from the
# id key on read and stored in no column, so a round trip under a fresh key
# would produce a database where every row is right and every public id is
# different, and `restore` refuses exactly that. A real deployment's key lives
# in its secret store and does not travel with its backups; a throwaway
# instance has nowhere else to put it. Treat the directory as a secret.
#
# Overrides, for the test that proves two of these coexist:
#
#   RN_SITE_EPH_BINARY   an already-built rn-site, instead of building here
#   RN_SITE_EPH_LOG      RUST_LOG for the instance (default rn_site=info,warn)
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

# The slot: stable for a path, uniform enough across paths that two worktrees
# beside each other do not land on the same number. `shasum` is on macOS and
# on the Linux images; `sha256sum` is the GNU spelling.
digest() {
    if command -v shasum >/dev/null 2>&1; then
        printf '%s' "$1" | shasum -a 256 | cut -c1-6
    else
        printf '%s' "$1" | sha256sum | cut -c1-6
    fi
}
slot=$((16#$(digest "$root") % 100))

app_port=$((3300 + slot))
admin_port=$((3400 + slot))
raft_port=$((8300 + slot))
api_port=$((8400 + slot))

state=$root/target/ephemeral
pid_file=$state/rn-site.pid
log_file=$state/rn-site.log
id_key_file=$state/id.key
oidc_key_file=$state/oidc.key
db_path=$state/data/db.sqlite
origin="http://127.0.0.1:$app_port"

binary=${RN_SITE_EPH_BINARY:-$root/target/debug/rn-site}

log() { printf '==> %s\n' "$*"; }

# A random hex secret of `$1` bytes, written to `$2` with owner-only mode and
# never printed. openssl is everywhere these ports are; /dev/urandom is the
# fallback that needs nothing at all.
mint_key() {
    local bytes=$1 path=$2
    [ -s "$path" ] && return 0
    mkdir -p "$(dirname "$path")"
    ( umask 077
      if command -v openssl >/dev/null 2>&1; then
          openssl rand -hex "$bytes" > "$path"
      else
          od -An -vtx1 -N "$bytes" /dev/urandom | tr -d ' \n' > "$path"
          printf '\n' >> "$path"
      fi )
    chmod 600 "$path"
}

running_pid() {
    [ -f "$pid_file" ] || return 1
    local pid
    pid=$(cat "$pid_file")
    [ -n "$pid" ] || return 1
    kill -0 "$pid" 2>/dev/null || return 1
    echo "$pid"
}

port_taken() {
    # A listener that is not ours. `lsof` answers on macOS and Linux alike;
    # when it is missing the bind failure below is the answer instead.
    command -v lsof >/dev/null 2>&1 || return 1
    lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1
}

cmd_up() {
    if pid=$(running_pid); then
        log "already up (pid $pid) at $origin"
        return 0
    fi
    if [ ! -x "$binary" ]; then
        log "building rn-site (debug)"
        CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-6} cargo build --quiet -p rn-site
    fi
    for port in "$app_port" "$raft_port" "$api_port"; do
        if port_taken "$port"; then
            echo "ephemeral: port $port is already listening and it is not this worktree's" >&2
            echo "ephemeral: run 'tools/ephemeral.sh status', or free the port" >&2
            exit 1
        fi
    done

    mkdir -p "$state/data"
    mint_key 16 "$id_key_file"
    mint_key 32 "$oidc_key_file"

    RN_SITE__MODE=dev \
    RN_SITE__DEV=1 \
    RN_SITE__ADDR="127.0.0.1:$app_port" \
    RN_SITE__ADMIN_ADDR="127.0.0.1:$admin_port" \
    RN_SITE__DB_PATH="$db_path" \
    RN_SITE__STATIC_DIR="$root/static" \
    RN_SITE__PUBLIC_ORIGIN="$origin" \
    RN_SITE__ID_KEY_FILE="$id_key_file" \
    RN_SITE__OIDC_KEY_FILE="$oidc_key_file" \
    RN_SITE_HQL_ADDR_RAFT="127.0.0.1:$raft_port" \
    RN_SITE_HQL_ADDR_API="127.0.0.1:$api_port" \
    RUST_LOG="${RN_SITE_EPH_LOG:-rn_site=info,warn}" \
        "$binary" >"$log_file" 2>&1 &
    echo $! > "$pid_file"
    local pid
    pid=$(cat "$pid_file")

    local deadline=$((SECONDS + 60))
    while [ "$SECONDS" -lt "$deadline" ]; do
        if curl --fail --silent --max-time 2 "$origin/readyz" | grep -q '"status":"ok"'; then
            log "slot $slot  pid $pid  app $app_port  admin $admin_port  raft $raft_port  hiqlite-api $api_port"
            log "up at $origin"
            log "sign in at $origin/auth — the button under the form opens an operator session"
            return 0
        fi
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.3
    done
    echo "ephemeral: the instance never became ready; see $log_file" >&2
    cmd_stop || true
    exit 1
}

cmd_status() {
    local pid state_line
    pid=$(running_pid || true)
    state_line=$(curl --fail --silent --max-time 2 "$origin/readyz" || echo '{"status":"unreachable"}')
    printf 'slot %-3s pid %-8s %s\n' "$slot" "${pid:--}" "$origin"
    printf '         %s\n' "$state_line"
    printf '         state %s\n' "$state"
}

cmd_stop() {
    local pid
    if ! pid=$(running_pid); then
        rm -f "$pid_file"
        log "not running"
        return 0
    fi
    # SIGTERM: the process drains its listener and hands the raft group back,
    # which is what leaves the data directory openable by the next start.
    kill "$pid"
    local deadline=$((SECONDS + 30))
    while kill -0 "$pid" 2>/dev/null && [ "$SECONDS" -lt "$deadline" ]; do
        sleep 0.3
    done
    if kill -0 "$pid" 2>/dev/null; then
        log "pid $pid ignored SIGTERM for 30s; killing it"
        kill -9 "$pid" 2>/dev/null || true
        sleep 1
    fi
    rm -f "$pid_file"
    log "stopped (slot $slot)"
}

cmd_reset() {
    cmd_stop
    rm -rf "$state"
    log "state removed: $state"
}

# `rn-site admin ...` against this worktree's instance, with the same
# environment `up` serves under. It refuses while the instance is running and
# names the pid, which is the point — so this stops nothing on its own.
cmd_admin() {
    [ -x "$binary" ] || cargo build --quiet -p rn-site
    mint_key 16 "$id_key_file"
    mint_key 32 "$oidc_key_file"
    RN_SITE__MODE=dev \
    RN_SITE__DEV=1 \
    RN_SITE__DB_PATH="$db_path" \
    RN_SITE__STATIC_DIR="$root/static" \
    RN_SITE__PUBLIC_ORIGIN="$origin" \
    RN_SITE__ID_KEY_FILE="$id_key_file" \
    RN_SITE__OIDC_KEY_FILE="$oidc_key_file" \
    RN_SITE_HQL_ADDR_RAFT="127.0.0.1:$raft_port" \
    RN_SITE_HQL_ADDR_API="127.0.0.1:$api_port" \
    RUST_LOG="${RN_SITE_EPH_LOG:-warn}" \
        "$binary" admin "$@"
}

# Stop, walk the database out, and leave it stopped: whoever asked for a
# backup is in the middle of something.
cmd_backup() {
    local dir=${1:-$state/backups/$(date -u +%Y%m%dT%H%M%SZ)}
    cmd_stop
    mkdir -p "$dir"
    cmd_admin backup "$dir"
    # The keys, which a `reset` would destroy and which the manifest
    # deliberately does not carry. See the header.
    ( umask 077; mkdir -p "$dir/keys" )
    cp "$id_key_file" "$dir/keys/id.key"
    cp "$oidc_key_file" "$dir/keys/oidc.key"
    chmod 600 "$dir/keys/id.key" "$dir/keys/oidc.key"
    log "keys copied into $dir/keys — treat this directory as a secret"
    log "backup complete: $dir"
}

# Wipe, put the keys back, then put the rows back. In that order: the restore
# refuses an id-key fingerprint that is not the manifest's, so the key has to
# be in place before the subcommand runs.
cmd_restore() {
    local dir=${1:?usage: ephemeral.sh restore <dir>}
    [ -f "$dir/manifest.json" ] || { echo "ephemeral: $dir holds no manifest.json" >&2; exit 1; }
    cmd_reset
    mkdir -p "$state/data"
    if [ -f "$dir/keys/id.key" ]; then
        ( umask 077
          cp "$dir/keys/id.key" "$id_key_file"
          cp "$dir/keys/oidc.key" "$oidc_key_file" )
        chmod 600 "$id_key_file" "$oidc_key_file"
        log "keys restored from $dir/keys"
    else
        log "no keys in $dir/keys; the restore will refuse the id-key mismatch"
    fi
    cmd_admin restore "$dir"
    log "restored from $dir — 'tools/ephemeral.sh up' to serve it"
}

case "${1:-up}" in
    up|start) cmd_up ;;
    backup) shift; cmd_backup "$@" ;;
    restore) shift; cmd_restore "$@" ;;
    admin) shift; cmd_admin "$@" ;;
    status) cmd_status ;;
    stop|down) cmd_stop ;;
    reset) cmd_reset ;;
    ports) printf 'slot %s app %s admin %s raft %s api %s\n' \
        "$slot" "$app_port" "$admin_port" "$raft_port" "$api_port" ;;
    *)
        sed -n '2,11p' "${BASH_SOURCE[0]}" >&2
        exit 2
        ;;
esac
