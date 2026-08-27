#!/usr/bin/env bash
# Prove every configured voter is application-ready before/after a rollout, and
# that the public surface the CDN serves is the one this revision builds.
#
#   deploy/verify-cluster.sh                 preflight: readiness only
#   deploy/verify-cluster.sh <git sha>       post-deploy: readiness + revision
#                                            + the public surface
#
# Two rules this script exists to enforce, both paid for once already
# (procedures/ha-service.md):
#
#   * Readiness names BOTH raft groups. hiqlite runs a sqlite group and a cache
#     group with independent membership; a 503 that reported `voters:3` and
#     said nothing about the cache group was unreadable for an afternoon.
#   * The public assertions go through the CDN, never at the origin. Cloudflare
#     overrode rn-site's `Cache-Control` on a MISS until the zone got a cache
#     rule, so an origin-side freshness contract is not what a visitor gets.
#
# Dependencies are curl, grep and tr, and nothing else — the CD runner's systemd
# unit carries exactly the tools its `path = [...]` names, and the first ever run
# of this script died on `curl: command not found`. Adding a dependency here
# means rebuilding that runner first.
set -euo pipefail

expected_revision="${1:-}"
origin="${RN_SITE_PUBLIC_ORIGIN:-https://ronitnath.com}"

nodes=(
    "nexus:100.88.31.199"
    "nyc:100.88.223.144"
    "delenda:100.88.252.202"
)

# Every raft group must be fully formed, on both groups, on every node.
expected_voters=3

fail() {
    echo "$*" >&2
    exit 1
}

# The body of one JSON object nested directly under "<name>": — enough to assert
# on a group without a JSON parser the runner may not have.
group_of() {
    local body="$1" name="$2" rest
    rest="${body#*\"$name\":\{}"
    [ "$rest" != "$body" ] || return 1
    printf '%s' "${rest%%\}*}"
}

# The status code, or 000 when the request never completed. Deliberately not
# fatal on its own: the caller says which code it wanted, and "expected 404,
# got 000" is a more useful line than curl's exit status under `set -e`.
http_status() {
    curl --silent --show-error --location --max-time 10 \
        --output /dev/null --write-out '%{http_code}' "$1" || true
}

# --- every voter, on both raft groups ---------------------------------------
for entry in "${nodes[@]}"; do
    IFS=: read -r name address <<<"$entry"

    # The published port is bound to the node's mesh IP, never loopback, so a
    # 127.0.0.1 gate would fail here while Compose called the container healthy.
    body=$(curl --fail --silent --show-error --max-time 5 \
        "http://$address:3160/readyz") \
        || fail "$name did not answer /readyz"

    case "$body" in
        *'"status":"ok"'*) ;;
        *) fail "$name is not ready: $body" ;;
    esac
    case "$body" in
        *"\"node\":\"$name\""*) ;;
        *) fail "$name reports a different node identity — check its node.env: $body" ;;
    esac

    for raft_group in sqlite cache; do
        group=$(group_of "$body" "$raft_group") \
            || fail "$name readiness has no $raft_group raft group: $body"
        case "$group" in
            *'"healthy":true'*) ;;
            *) fail "$name $raft_group raft group is unhealthy: $group" ;;
        esac
        case "$group" in
            *"\"voters\":$expected_voters"*) ;;
            *) fail "$name $raft_group raft group is not $expected_voters voters: $group" ;;
        esac
        case "$group" in
            *"\"expected_voters\":$expected_voters"*) ;;
            *) fail "$name $raft_group raft group expects a different size — its peer map disagrees with the fleet: $group" ;;
        esac
    done

    if [ -n "$expected_revision" ]; then
        case "$body" in
            *"\"version\":\"$expected_revision\""*) ;;
            *) fail "$name is not serving $expected_revision: $body" ;;
        esac
    fi

    echo "$name ready: $body"
done

[ -n "$expected_revision" ] || exit 0

# --- the public surface, through the CDN -------------------------------------
for path in / /tokens.css; do
    status=$(http_status "$origin$path")
    [ "$status" = 200 ] || fail "$origin$path answered $status, expected 200"
    echo "$path 200"
done

# The freshness contract, proven where it actually has to hold. Stable asset
# URLs mean the browser must revalidate rather than cache: there is no `?v=`
# cache-busting anywhere in this application, by design.
asset_headers=$(curl --fail --silent --show-error --head --max-time 10 "$origin/tokens.css") \
    || fail "$origin/tokens.css has no headers to read"
case "$asset_headers" in
    *'cache-control: no-cache, must-revalidate'*) ;;
    *) fail "public assets do not require revalidation: $asset_headers" ;;
esac
case "$asset_headers" in
    *"x-rn-app-version: $expected_revision"*) ;;
    *) fail "public asset version header does not match $expected_revision: $asset_headers" ;;
esac
echo "/tokens.css revalidates and is stamped $expected_revision"

# One real asset per bundle. Trunk fingerprints the three tier bundles, so the
# names cannot be written down here — they are read out of the index trunk
# emitted beside them, which is the single statement of what a bundle loads.
# A bundle that built empty answers this with a 404 and stops the rollout.
for prefix in /app/pkg /org/pkg /platform/pkg /pkg/starscape; do
    index=$(curl --fail --silent --show-error --max-time 10 "$origin$prefix/index.html") \
        || fail "$prefix/index.html is not served — that bundle is missing from the image"
    # Trunk writes its module in a single-quoted import and its preloads in
    # double-quoted attributes; split on both so either form is found.
    asset=$(printf '%s' "$index" | tr "\"'" '\n\n' \
        | grep -m1 -E "^$prefix/.*\.(js|wasm)$" || true)
    [ -n "$asset" ] || fail "$prefix/index.html names no module — the bundle built empty"
    status=$(http_status "$origin$asset")
    [ "$status" = 200 ] || fail "$origin$asset answered $status, expected 200"
    echo "$asset 200"
done

# --- what must NOT exist -----------------------------------------------------
# The deleted surfaces, asserted as absent rather than assumed gone: the islands
# hydration module, the old invalidation hub, and the legacy operator page. A
# release that resurrects any of them is a merge accident, and this is the only
# place it would be noticed.
for path in /api/realtime /manage /pkg/rn-site.wasm; do
    status=$(http_status "$origin$path")
    [ "$status" = 404 ] || fail "$origin$path answered $status, expected 404 — a deleted surface is being served"
    echo "$path 404"
done

echo "cluster verified on $expected_revision"
