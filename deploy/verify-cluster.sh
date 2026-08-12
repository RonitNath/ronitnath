#!/usr/bin/env bash
# Prove every configured voter is application-ready before/after a rollout.
# An expected revision is optional for the preflight and mandatory post-deploy.
set -euo pipefail

expected_revision="${1:-}"
nodes=(
    "nexus:100.88.31.199"
    "nyc:100.88.223.144"
    "delenda:100.88.252.202"
)

for entry in "${nodes[@]}"; do
    IFS=: read -r name address <<<"$entry"
    body=$(curl --fail --silent --show-error --max-time 5 \
        "http://$address:3160/readyz")

    if [[ "$body" != *'"status":"ok"'* ]]; then
        echo "$name is not database-ready: $body" >&2
        exit 1
    fi
    if [[ -n "$expected_revision" && "$body" != *"\"version\":\"$expected_revision\""* ]]; then
        echo "$name is not serving expected revision $expected_revision: $body" >&2
        exit 1
    fi
    if [[ -n "$expected_revision" \
        && ("$body" != *'"voters":3'* || "$body" != *'"expected_voters":3'*) ]]; then
        echo "$name has not converged on the three-voter topology: $body" >&2
        exit 1
    fi
    if [[ -n "$expected_revision" && "$body" != *'"realtime_listener":true'* ]]; then
        echo "$name realtime listener is not healthy: $body" >&2
        exit 1
    fi
    echo "$name ready: $body"
done

if [[ -n "$expected_revision" ]]; then
    asset_headers=$(curl --fail --silent --show-error --head --max-time 10 \
        https://ronitnath.com/css/site.css)
    if [[ "$asset_headers" != *'cache-control: no-cache, must-revalidate'* ]]; then
        echo "public assets do not require revalidation: $asset_headers" >&2
        exit 1
    fi
    if [[ "$asset_headers" != *"x-rn-app-version: $expected_revision"* ]]; then
        echo "public asset version header does not match $expected_revision" >&2
        exit 1
    fi

    html=$(curl --fail --silent --show-error --max-time 10 https://ronitnath.com/)
    if [[ "$html" == *'?v='* ]]; then
        echo "public HTML still contains query-string asset cache busting" >&2
        exit 1
    fi
    if [[ "$html" != *'/pkg/rn-site.wasm'* || "$html" == *'/pkg/rn-site_bg.wasm'* ]]; then
        echo "public HTML does not reference the packaged hydration module" >&2
        exit 1
    fi

    wasm_headers=$(curl --fail --silent --show-error --head --max-time 10 \
        https://ronitnath.com/pkg/rn-site.wasm)
    if [[ "$wasm_headers" != *'content-type: application/wasm'* ]]; then
        echo "public hydration module is unavailable or has the wrong content type" >&2
        exit 1
    fi
    if [[ "$wasm_headers" != *'cache-control: no-cache, must-revalidate'* ]]; then
        echo "public hydration module does not require revalidation" >&2
        exit 1
    fi

    ws_headers=$(curl --silent --show-error --http1.1 --max-time 2 \
        --output /dev/null --dump-header - \
        -H 'Origin: https://ronitnath.com' \
        -H 'Connection: Upgrade' \
        -H 'Upgrade: websocket' \
        -H 'Sec-WebSocket-Version: 13' \
        -H 'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==' \
        https://ronitnath.com/api/realtime || true)
    if [[ "$ws_headers" != *' 101 '* ]]; then
        echo "public realtime endpoint did not upgrade: $ws_headers" >&2
        exit 1
    fi
fi
