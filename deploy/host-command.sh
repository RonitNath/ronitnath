#!/usr/bin/env bash
set -euo pipefail

lock=/run/lock/ronitnath-deploy.lock
state=/data/crypt/ronitnath/releases
mkdir -p "$state"
chmod 700 "$state"

read -r action sha image migration extra <<<"${SSH_ORIGINAL_COMMAND:-}"
[[ "${sha:-}" =~ ^[a-f0-9]{40}$ ]] || { echo 'invalid candidate' >&2; exit 64; }
image_re='^ghcr\.io/ronitnath/ronitnath-app@sha256:[a-f0-9]{64}$'

case "${action:-}" in
  preflight)
    [[ "$image" =~ $image_re && "$migration" =~ $image_re && -z "${extra:-}" ]] || exit 64
    flock "$lock" docker pull "$image"
    flock "$lock" docker pull "$migration"
    available=$(df --output=avail -B1 /data | tail -1)
    (( available >= 5368709120 )) || { echo 'less than 5 GiB free on /data' >&2; exit 70; }
    previous=$(docker inspect --format '{{.Image}}' ronitnath-web 2>/dev/null || true)
    printf '{"requestedSha":"%s","previousImageId":"%s","runtime":"%s","migrate":"%s","at":"%s"}\n' "$sha" "$previous" "$image" "$migration" "$(date -u +%FT%TZ)" > "$state/$sha.pending"
    mv "$state/$sha.pending" "$state/$sha.json"
    ;;
  migrate)
    [[ "$image" =~ $image_re && -z "${migration:-}" ]] || exit 64
    flock "$lock" docker run --rm --network host --env-file /data/crypt/ronitnath/web.env "$image"
    ;;
  roll)
    [[ "$image" =~ $image_re && -z "${migration:-}" ]] || exit 64
    flock "$lock" env IMAGE="$image" APP_VERSION="$sha" docker compose -f /data/crypt/ronitnath/compose.yaml up -d --wait --wait-timeout 120
    curl -fsS http://127.0.0.1:3140/readyz | grep -F "\"version\":\"$sha\"" >/dev/null
    ;;
  state)
    docker inspect --format '{{json .}}' ronitnath-web
    ;;
  *) echo 'unsupported deployment command' >&2; exit 64 ;;
esac
