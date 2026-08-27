#!/usr/bin/env bash
# The resilience drill: kill a voter under load, kill a second, put them back.
#
#   tools/perf/drill.sh                 two rounds, into docs/perf/<today>/
#   tools/perf/drill.sh --rounds 1      one
#   tools/perf/drill.sh --keep          leave the cluster up afterwards
#
# The three claims under test, in the order the plan states them:
#
#   one voter down   commits continue; count the errors and the stall
#   two voters down  writes refuse — a decline, not a 500 and not a hang —
#                    and reads on the survivor still answer
#   restarted        the cluster converges; /readyz reports three voters and a
#                    command commits
#
# The load is pointed at a node that is *not* the leader, and the leader is the
# first one killed. Killing a follower proves nothing a reader would doubt:
# the interesting stall is the election, and the interesting survivor is the
# one still holding the client's connections while it happens.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

rounds=2
seconds=150
keep=0
documents=${RN_PERF_DRILL_DOCUMENTS:-500}
work=$root/target/perf
out=$root/docs/perf/$(date +%F)

while [ $# -gt 0 ]; do
    case "$1" in
        --rounds) rounds=$2; shift 2 ;;
        --seconds) seconds=$2; shift 2 ;;
        --documents) documents=$2; shift 2 ;;
        --keep) keep=1; shift ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done

mkdir -p "$out" "$work"
perf=$root/tools/perf/target/release/rn-perf
log() { printf '==> %s\n' "$*"; }
port_of() { echo $((3160 + $1)); }
stamp() { date +%s.%N; }

# What one node says about itself. Empty when it is not answering.
readyz() { curl --fail --silent --max-time 2 "http://127.0.0.1:$(port_of "$1")/readyz" || true; }

# The node id the sqlite group currently calls leader, as seen from $1.
leader_seen_by() {
    readyz "$1" | sed -n 's/.*"sqlite":{[^}]*"leader":\([0-9]*\).*/\1/p'
}

cleanup() {
    if [ -n "${load_pid:-}" ]; then kill "$load_pid" 2>/dev/null || true; fi
    if [ -n "${reads_pid:-}" ]; then kill "$reads_pid" 2>/dev/null || true; fi
    if [ "$keep" -eq 0 ]; then
        tools/cluster.sh stop >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

log "building the server and the harness"
CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4} cargo build --quiet -p rn-site
(cd tools/perf && CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4} cargo build --quiet --release)

if [ -z "$(readyz 1)" ]; then
    log "starting the cluster"
    tools/cluster.sh start
fi

# A world small enough that a re-seed between rounds is not the drill's cost.
log "seeding the drill's world"
"$perf" seed --host "127.0.0.1:$(port_of 1)" --documents "$documents" \
    --subscribers 2 --workers 8 --out "$work/drill-seed.json" >/dev/null
cookie=$(cat "$work/owner.cookie")

for round in $(seq 1 "$rounds"); do
    report=$out/drill-$round.txt
    : > "$report"
    say() { printf '%s\n' "$*" | tee -a "$report"; }

    leader=$(leader_seen_by 1)
    [ -n "$leader" ] || leader=1
    survivor=$( [ "$leader" = 1 ] && echo 2 || echo 1 )
    third=$(for id in 1 2 3; do [ "$id" != "$leader" ] && [ "$id" != "$survivor" ] && echo "$id"; done)
    host=127.0.0.1:$(port_of "$survivor")

    say "round $round — $(date -Iseconds)"
    say "  leader $leader, load and reads on node $survivor, second kill node $third"
    say "  load average at start: $(uptime | sed -E 's/.*load averages?: *//')"

    zero=$(stamp)
    at() { awk "BEGIN{printf \"t+%.1fs\", $(stamp) - $zero}"; }

    "$perf" commit --host "$host" --seed "$work/drill-seed.json" \
        --seconds "$seconds" --reads 2 --timeline --persist \
        --json "$out/drill-$round.json" > "$out/drill-$round-load.txt" 2>&1 &
    load_pid=$!

    sleep 25
    say "  $(at)  kill node $leader (the leader) — one voter down"
    tools/cluster.sh kill "$leader" >> "$report" 2>&1

    sleep 45
    say "  $(at)  kill node $third — two voters down, no quorum"
    tools/cluster.sh kill "$third" >> "$report" 2>&1

    # Reads on the survivor, while there is no quorum to write with. With the
    # session cookie: an anonymous read is a 403 whatever the cluster is
    # doing, and proving the decline works is not the claim under test.
    say "  $(at)  reads on the survivor with no quorum:"
    (
        for _ in $(seq 20); do
            curl --silent --output /dev/null --max-time 15 \
                --header "cookie: rn_session=$cookie" \
                --write-out '    read /api/q/documents %{http_code} in %{time_total}s\n' \
                "http://$host/api/q/documents" || echo '    read failed'
            sleep 1
        done
    ) >> "$report" 2>&1 &
    reads_pid=$!
    wait "$reads_pid" || true; reads_pid=

    say "  $(at)  restarting the cluster"
    restart=$(stamp)
    # `cluster.sh start` blocks until it is satisfied or gives up after 90s, so
    # the convergence clock cannot be read off its return: it runs in the
    # background and this polls at 200 ms, which is the resolution the claim
    # deserves. A node that does not come back gets one retry and, either way,
    # the last line of its log.
    tools/cluster.sh start >> "$report" 2>&1 &
    starter=$!
    converged=""
    retried=0
    poll_deadline=$((SECONDS + 240))
    while [ "$SECONDS" -lt "$poll_deadline" ]; do
        up=0
        for id in 1 2 3; do
            case "$(readyz "$id")" in
                *'"status":"ok"'*'"voters":3'*) up=$((up + 1)) ;;
            esac
        done
        if [ "$up" -eq 3 ]; then
            converged=$(awk "BEGIN{printf \"%.1f\", $(stamp) - $restart}")
            break
        fi
        if ! kill -0 "$starter" 2>/dev/null && [ "$retried" -eq 0 ]; then
            retried=1
            say "  $(at)  a node did not come back on the first start; retrying once"
            tools/cluster.sh start >> "$report" 2>&1 &
            starter=$!
        fi
        sleep 0.2
    done
    wait "$starter" 2>/dev/null || true

    if [ -n "$converged" ]; then
        say "  $(at)  three voters, all /readyz ok, ${converged}s after the restart was issued (retries: $retried)"
    else
        say "  $(at)  the cluster did NOT reconverge within 240s (retries: $retried)"
    fi
    for id in 1 2 3; do
        say "    node $id: $(readyz "$id")"
        if [ -z "$(readyz "$id")" ]; then
            say "      last log line: $(tail -1 "target/cluster/node$id/rn-site.log" 2>/dev/null)"
        fi
    done

    # Converged is not the same as writable: the drill's last claim is that a
    # command commits, so it commits one and times it.
    committed=$(stamp)
    if "$perf" commit --host "$host" --seed "$work/drill-seed.json" --seconds 2 --reads 0 \
        >> "$report" 2>&1; then
        say "  $(at)  a command committed $(awk "BEGIN{printf \"%.1f\", $(stamp) - $committed}")s after the restart returned"
    else
        say "  $(at)  a command did NOT commit after the restart — see $report"
    fi

    wait "$load_pid" || true
    load_pid=
    say "  timeline (one row per second of the load):"
    grep -E '^  t\+' "$out/drill-$round-load.txt" >> "$report" || true
    say "  round $round done"
    say ""
done

log "drill written to $out/drill-*.txt"
