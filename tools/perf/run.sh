#!/usr/bin/env bash
# The perf harness: a three-voter cluster, a seeded world, and every profile
# the kernel report puts a budget on.
#
#   tools/perf/run.sh                     the whole thing, into docs/perf/<today>/
#   tools/perf/run.sh --documents 2000    a smaller world, for a smoke run
#   tools/perf/run.sh --keep              leave the cluster up afterwards
#
# What it produces is a directory of raw evidence — one file per profile, oha's
# JSON beside its table, the flame graph, the machine and the revision — which
# `docs/perf/<date>.md` then quotes. The raw files are the record; the markdown
# is the reading of it.
#
# Two rules this script exists to enforce:
#
#   * nothing is measured while anything is compiling. A profile that overlaps
#     a `cargo build` measures the compiler. `quiet` blocks until no cargo,
#     rustc or trunk is running and the one-minute load average is under the
#     ceiling, and every profile records the load average it actually started
#     at, so a reader can see the condition rather than trust it.
#   * the world is seeded through the real API. See tools/perf/src/seed.rs.
#
# The flame graphs come from a separate single-node server started *by* samply
# (`--samply`, default on): attaching to a running process needs root on macOS,
# and a profile taken under sudo is not a profile anybody will re-run. The
# frames are the same binary's; what the single node does not show is the raft
# round trip, which is exactly what the cluster profiles are for.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

documents=10000
subscribers=100
seconds=30
workers=8
node=127.0.0.1:3161
samply_node=127.0.0.1:3164
keep=0
do_samply=1
load_ceiling=${RN_PERF_LOAD_CEILING:-4}
quiet_budget=${RN_PERF_QUIET_BUDGET:-5400}

while [ $# -gt 0 ]; do
    case "$1" in
        --documents) documents=$2; shift 2 ;;
        --subscribers) subscribers=$2; shift 2 ;;
        --seconds) seconds=$2; shift 2 ;;
        --workers) workers=$2; shift 2 ;;
        --keep) keep=1; shift ;;
        --no-samply) do_samply=0; shift ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done

date_stamp=$(date +%F)
out=$root/docs/perf/$date_stamp
work=$root/target/perf
mkdir -p "$out" "$work"

log() { printf '==> %s\n' "$*"; }

# The one-minute load average, as a number this shell can compare.
load_now() {
    uptime | sed -E 's/.*load averages?: *//' | awk '{print $1}' | tr -d ','
}

# Block until the machine is idle enough to measure on. Sibling legs compile in
# their own worktrees; this waits them out rather than racing them.
quiet() {
    local waited=0
    while [ "$waited" -lt "$quiet_budget" ]; do
        local busy load
        busy=$(pgrep -x cargo rustc trunk 2>/dev/null | wc -l | tr -d ' ')
        load=$(load_now)
        if [ "$busy" -eq 0 ] && awk "BEGIN{exit !($load < $load_ceiling)}"; then
            return 0
        fi
        printf '    waiting for a quiet machine (load %s, %s compiler process(es))\n' \
            "$load" "$busy"
        sleep 60
        waited=$((waited + 60))
    done
    log "the machine never went quiet within ${quiet_budget}s; measuring anyway and saying so"
    return 1
}

# Run one oha profile, with the load average it started at recorded beside it.
# $1 name  $2 url  $3.. extra oha arguments
oha_profile() {
    local name=$1 url=$2
    shift 2
    local load
    quiet || true
    load=$(load_now)
    log "profile $name (load average at start: $load)"
    {
        printf 'profile: %s\nurl: %s\nload average at start: %s\nstarted: %s\n\n' \
            "$name" "$url" "$load" "$(date -Iseconds)"
    } > "$out/$name.txt"
    oha --no-tui -c 8 -z "${seconds}s" "$@" "$url" >> "$out/$name.txt" 2>&1
    oha --no-tui -c 8 -z 5s -j "$@" "$url" > "$out/$name.json" 2>/dev/null || true
    printf '    %s\n' "$(grep -m1 'Requests/sec' "$out/$name.txt" || true)"
}

cleanup() {
    # The profiled node always goes, `--keep` or not: it is this script's own
    # process and nothing else in the run refers to it.
    if [ -n "${samply_pid:-}" ]; then
        leftover=$(lsof -ti "tcp:${samply_node##*:}" -sTCP:LISTEN 2>/dev/null | head -1)
        [ -n "$leftover" ] && kill "$leftover" 2>/dev/null
        kill "$samply_pid" 2>/dev/null || true
        wait "$samply_pid" 2>/dev/null || true
    fi
    rm -rf "$work/samply-node"
    if [ "$keep" -eq 0 ]; then
        log "stopping the cluster"
        tools/cluster.sh stop >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

# ---------------------------------------------------------------- build first
# Everything that compiles happens before anything that measures, so `quiet`
# has something to wait for rather than something to race.
#
# The server embeds each bundle's `dist/` with rust-embed, which is a compile
# error rather than an empty tree when the directory is missing — so a perf run
# in a fresh worktree needs `trunk build` to have happened, and says which one
# is missing rather than handing back four pages of macro output.
for bundle in starscape app-member app-org app-platform; do
    if [ ! -d "crates/$bundle/dist" ]; then
        echo "crates/$bundle/dist is missing — run 'trunk build --release' in it first" >&2
        exit 1
    fi
done

log "building the server and the harness"
CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4} cargo build --quiet -p rn-site
(cd tools/perf && CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4} cargo build --quiet --release)
perf=$root/tools/perf/target/release/rn-perf

{
    printf 'revision: %s\n' "$(git rev-parse HEAD)"
    printf 'branch:   %s\n' "$(git rev-parse --abbrev-ref HEAD)"
    printf 'date:     %s\n' "$(date -Iseconds)"
    printf 'machine:  %s\n' "$(uname -mrs)"
    printf 'cpu:      %s (%s cores)\n' \
        "$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown)" \
        "$(sysctl -n hw.ncpu 2>/dev/null || echo '?')"
    printf 'memory:   %s bytes\n' "$(sysctl -n hw.memsize 2>/dev/null || echo '?')"
    printf 'rustc:    %s\n' "$(rustc --version)"
    printf 'oha:      %s\n' "$(oha --version)"
    printf 'samply:   %s\n' "$(samply --version 2>/dev/null || echo absent)"
    printf 'seed:     %s documents, %s subscribers, %s commit workers\n' \
        "$documents" "$subscribers" "$workers"
    # Two of the report's budgets are stated as "one RTT plus a millisecond",
    # so the run has to say what an RTT is here. On this harness it is the
    # loopback, which is the floor a real intra-zone number sits above.
    printf 'loopback: %s\n' \
        "$(ping -c 20 -q 127.0.0.1 2>/dev/null | tail -1 || echo 'not measured')"
} > "$out/run.txt"
log "wrote $out/run.txt"

# ---------------------------------------------------------------- the cluster
log "starting the three-voter cluster"
tools/cluster.sh start
tools/cluster.sh status > "$out/cluster-before.txt"

log "seeding $documents documents and $subscribers subscribers through the API"
"$perf" seed --host "$node" --documents "$documents" --subscribers "$subscribers" \
    --workers "$workers" --out "$work/seed.json" 2>&1 | tee "$out/seed.txt"
cookie=$(cat "$work/owner.cookie")

# ---------------------------------------------------------------- oha profiles
oha_profile whoami "http://$node/api/whoami" -H "cookie: rn_session=$cookie"
oha_profile documents "http://$node/api/q/documents" -H "cookie: rn_session=$cookie"
oha_profile landing "http://$node/"

# ------------------------------------------------------- commit and fan-out
quiet || true
log "commit profile (load average at start: $(load_now))"
{
    printf 'profile: commit\nload average at start: %s\nstarted: %s\n\n' \
        "$(load_now)" "$(date -Iseconds)"
} > "$out/commit.txt"
"$perf" commit --seed "$work/seed.json" --seconds "$seconds" --reads 4 \
    --json "$out/commit.json" 2>&1 | tee -a "$out/commit.txt"

quiet || true
log "websocket fan-out probe (load average at start: $(load_now))"
{
    printf 'profile: fan-out\nload average at start: %s\nstarted: %s\n\n' \
        "$(load_now)" "$(date -Iseconds)"
} > "$out/fanout.txt"
"$perf" wsprobe --seed "$work/seed.json" --subscribers "$subscribers" \
    --json "$out/fanout.json" 2>&1 | tee -a "$out/fanout.txt"

tools/cluster.sh status > "$out/cluster-after.txt"

# ---------------------------------------------------------------- flame graphs
if [ "$do_samply" -eq 1 ] && command -v samply >/dev/null 2>&1; then
    quiet || true
    log "flame graphs: a single node under samply (load average $(load_now))"
    mkdir -p "$work/samply-node"
    RN_SITE__MODE=dev \
    RN_SITE__DB_PATH="$work/samply-node/db.sqlite" \
    RN_SITE__ADDR="$samply_node" \
    RN_SITE__STATIC_DIR="$root/static" \
    RN_SITE__ID_KEY=000102030405060708090a0b0c0d0e0f \
    RN_SITE_NODE=samply \
    RUST_LOG=warn \
        samply record --save-only --profile-name rn-site \
            -o "$out/flamegraph-whoami-and-commit.json.gz" \
            -- "$root/target/debug/rn-site" > "$work/samply-node/rn-site.log" 2>&1 &
    samply_pid=$!

    for _ in $(seq 90); do
        curl --fail --silent --max-time 2 "http://$samply_node/readyz" >/dev/null 2>&1 && break
        sleep 1
    done

    "$perf" seed --host "$samply_node" --documents "$documents" --subscribers 4 \
        --workers "$workers" --out "$work/samply-seed.json" >> "$out/seed.txt" 2>&1
    samply_cookie=$(cat "$work/owner.cookie")
    oha --no-tui -c 8 -z 20s -H "cookie: rn_session=$samply_cookie" \
        "http://$samply_node/api/whoami" > "$out/samply-whoami.txt" 2>&1
    "$perf" commit --seed "$work/samply-seed.json" --seconds 20 --reads 4 \
        > "$out/samply-commit.txt" 2>&1

    # samply writes the profile when the process *it started* exits, and dies
    # without writing anything if it is signalled itself — so the server is the
    # one that gets the signal. It is found by the port this script chose,
    # never by name: sibling legs run their own `rn-site` on this machine and
    # a pattern kill would take theirs with it.
    profiled=$(lsof -ti "tcp:${samply_node##*:}" -sTCP:LISTEN 2>/dev/null | head -1)
    if [ -n "$profiled" ]; then
        kill "$profiled" 2>/dev/null || true
    fi
    wait "$samply_pid" 2>/dev/null || true
    samply_pid=
    ls -lh "$out"/flamegraph-*.json.gz >> "$out/run.txt" 2>&1 || true
    log "flame graph at $out/flamegraph-whoami-and-commit.json.gz"

    if command -v python3 >/dev/null 2>&1; then
        python3 tools/perf/hot-frames.py "$out/flamegraph-whoami-and-commit.json.gz" \
            --binary "$root/target/debug/rn-site" --top 25 \
            > "$out/hot-frames.txt" 2>&1 || true
        log "hot frames at $out/hot-frames.txt"
    fi
else
    log "samply not run; the report says so"
fi

log "done — raw evidence in $out"
