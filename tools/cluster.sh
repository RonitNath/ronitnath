#!/usr/bin/env bash
# Three rn-site voters on this host, for the perf and resilience drills.
#
#   tools/cluster.sh start        build once, then boot nodes 1..3
#   tools/cluster.sh status       what each node reports on /readyz
#   tools/cluster.sh kill <n>     stop one node, leaving the other two
#   tools/cluster.sh stop         stop everything and remove the state
#
# This is a real three-voter raft cluster, not three unrelated processes: they
# share the peer map and the secrets, so killing one proves the quorum survives
# and killing two proves writes refuse. What it is not is production — one host,
# one disk, one power supply — which is why it says `RN_SITE_HQL_LOCAL_CLUSTER=1`
# and the deployed nodes never do.
#
# Ports, so nothing collides with a `cargo run` on 3004:
#
#   node  app    raft   hiqlite api
#   1     3161   8101   8201
#   2     3162   8102   8202
#   3     3163   8103   8203
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

run_dir=${RN_SITE_CLUSTER_DIR:-$root/target/cluster}
binary=$root/target/debug/rn-site
nodes=(1 2 3)

# The peer map every node agrees on. Loopback throughout: see the header.
peer_map='1@127.0.0.1:8101@127.0.0.1:8201,2@127.0.0.1:8102@127.0.0.1:8202,3@127.0.0.1:8103@127.0.0.1:8203'

# Not secret and not pretending to be: three processes on one loopback have no
# peer to authenticate. Long enough to satisfy the 16-character minimum.
secret_raft='rn-site-local-cluster-raft-secret'
secret_api='rn-site-local-cluster-api-secret'

app_port() { echo $((3160 + $1)); }
pid_file() { echo "$run_dir/node$1/rn-site.pid"; }

log() { printf '==> %s\n' "$*"; }

node_pid() {
    local file
    file=$(pid_file "$1")
    [ -f "$file" ] || return 1
    local pid
    pid=$(cat "$file")
    kill -0 "$pid" 2>/dev/null || return 1
    echo "$pid"
}

start_node() {
    local id=$1
    local dir=$run_dir/node$id
    if node_pid "$id" >/dev/null; then
        log "node $id is already running"
        return
    fi
    mkdir -p "$dir/data"

    # `prod` is the mode that reads the peer map at all; `dev` is single-node by
    # definition. An id_key is required in that mode for the same reason it is
    # in production, so the harness pins one rather than minting a new one per
    # node — three voters must derive the same public ids.
    RN_SITE__MODE=prod \
    RN_SITE__DB_PATH="$dir/data/db.sqlite" \
    RN_SITE__ADDR="127.0.0.1:$(app_port "$id")" \
    RN_SITE__STATIC_DIR="$root/static" \
    RN_SITE__ID_KEY=000102030405060708090a0b0c0d0e0f \
    RN_SITE_NODE="local-$id" \
    RN_SITE_HQL_NODE_ID="$id" \
    RN_SITE_HQL_NODES="$peer_map" \
    RN_SITE_HQL_ADDR_RAFT="127.0.0.1:810$id" \
    RN_SITE_HQL_ADDR_API="127.0.0.1:820$id" \
    RN_SITE_HQL_SECRET_RAFT="$secret_raft" \
    RN_SITE_HQL_SECRET_API="$secret_api" \
    RN_SITE_HQL_LOCAL_CLUSTER=1 \
    RUST_LOG="${RUST_LOG:-rn_site=info,warn}" \
        "$binary" >"$dir/rn-site.log" 2>&1 &
    echo $! > "$(pid_file "$id")"
    log "node $id started (pid $!, app 127.0.0.1:$(app_port "$id"), log $dir/rn-site.log)"
}

wait_for_ready() {
    # A cold three-voter formation waits out hiqlite's is-this-a-rejoin delay on
    # both raft groups before it elects, so the budget is generous on purpose.
    local deadline=$((SECONDS + 90))
    while [ "$SECONDS" -lt "$deadline" ]; do
        local ready=0
        for id in "${nodes[@]}"; do
            curl --fail --silent --max-time 2 "http://127.0.0.1:$(app_port "$id")/readyz" \
                | grep -q '"status":"ok"' && ready=$((ready + 1))
        done
        if [ "$ready" -eq "${#nodes[@]}" ]; then
            log "all ${#nodes[@]} nodes are ready"
            return 0
        fi
        sleep 1
    done
    log "gave up waiting; run 'tools/cluster.sh status' and read the node logs"
    return 1
}

cmd_start() {
    log "building rn-site"
    CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4} cargo build --quiet -p rn-site
    mkdir -p "$run_dir"
    for id in "${nodes[@]}"; do
        start_node "$id"
    done
    wait_for_ready
}

cmd_status() {
    for id in "${nodes[@]}"; do
        local pid state
        pid=$(node_pid "$id" || true)
        state=$(curl --fail --silent --max-time 2 \
            "http://127.0.0.1:$(app_port "$id")/readyz" || echo '{"status":"unreachable"}')
        printf 'node %s  pid %-8s %s\n' "$id" "${pid:--}" "$state"
    done
}

cmd_kill() {
    local id=${1:?usage: cluster.sh kill <node>}
    local pid
    if ! pid=$(node_pid "$id"); then
        log "node $id is not running"
        return
    fi
    # SIGTERM, not SIGKILL: draining the voter is the behaviour under test.
    kill "$pid"
    wait_for_exit "$pid"
    rm -f "$(pid_file "$id")"
    log "node $id stopped"
}

wait_for_exit() {
    local pid=$1
    local deadline=$((SECONDS + 40))
    while kill -0 "$pid" 2>/dev/null && [ "$SECONDS" -lt "$deadline" ]; do
        sleep 1
    done
    if kill -0 "$pid" 2>/dev/null; then
        log "pid $pid ignored SIGTERM for 40s; killing it"
        kill -9 "$pid" 2>/dev/null || true
        sleep 1
    fi
}

cmd_stop() {
    for id in "${nodes[@]}"; do
        cmd_kill "$id" || true
    done
    # The state is disposable by construction: the cluster is formed fresh on
    # every start, and a half-formed raft directory left behind is poison for
    # the next one.
    rm -rf "$run_dir"
    log "cluster stopped and state removed"
}

case "${1:-}" in
    start) cmd_start ;;
    status) cmd_status ;;
    kill) shift; cmd_kill "$@" ;;
    stop) cmd_stop ;;
    *)
        sed -n '2,10p' "${BASH_SOURCE[0]}" >&2
        exit 2
        ;;
esac
