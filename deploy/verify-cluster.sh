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
    echo "$name ready: $body"
done
