#!/bin/sh
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
stamp="$$-$(date +%s)"
image="ronitnath-dbos-local:$stamp"
migrate_image="ronitnath-dbos-migrate-local:$stamp"
network="ronitnath-dbos-$stamp"
database="ronitnath-dbos-postgres-$stamp"
application="ronitnath-dbos-app-$stamp"
token_file=${NPM_TOKEN_FILE:-}
created_token=

cleanup() {
  docker rm -f "$application" >/dev/null 2>&1 || true
  docker rm -f "$database" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
  docker image rm "$image" "$migrate_image" >/dev/null 2>&1 || true
  if [ -n "$created_token" ]; then rm -f "$created_token"; fi
}
trap cleanup EXIT INT TERM

if [ -z "$token_file" ]; then
  created_token=$(mktemp "${TMPDIR:-/tmp}/ronitnath-npm-token.XXXXXX")
  gh auth token > "$created_token"
  chmod 600 "$created_token"
  token_file=$created_token
fi

docker build --secret "id=npm_token,src=$token_file" --build-arg APP_VERSION=container-test \
  --target runtime -t "$image" "$root"
docker build --secret "id=npm_token,src=$token_file" --build-arg APP_VERSION=container-test \
  --target migrate -t "$migrate_image" "$root"

docker network create "$network" >/dev/null
docker run -d --name "$database" --network "$network" \
  -e POSTGRES_PASSWORD=test -e POSTGRES_USER=test -e POSTGRES_DB=ronitnath postgres:17 >/dev/null
until docker exec "$database" pg_isready -U test -d ronitnath >/dev/null 2>&1; do sleep 1; done

database_url="postgres://test:test@$database:5432/ronitnath"
common_env="--env DATABASE_URL=$database_url --env DBOS_SYSTEM_DATABASE_URL=$database_url --env APP_VERSION=container-test --env AUTH_SECRET=container-test-auth-secret-at-least-32-characters --env ID_KEY=00112233445566778899aabbccddeeff --env MAIL_INTENT_KEY=container-test-mail-key --env PUBLIC_ORIGIN=http://127.0.0.1:3140"
# shellcheck disable=SC2086
docker run --rm --network "$network" $common_env "$migrate_image"

start_application() {
  start_ms=$(node -p 'Date.now()')
  docker rm -f "$application" >/dev/null 2>&1 || true
  # shellcheck disable=SC2086
  docker run -d --init --stop-timeout 60 --name "$application" --network "$network" \
    -p 127.0.0.1::3140 $common_env --env REPLICA_ID=container-a "$image" >/dev/null
  host_port=$(docker port "$application" 3140/tcp | sed 's/.*://')
  deadline=$(( $(date +%s) + 120 ))
  until curl -fsS "http://127.0.0.1:$host_port/readyz" >/dev/null 2>&1; do
    if [ "$(date +%s)" -ge "$deadline" ]; then docker logs "$application"; return 1; fi
    sleep 1
  done
  startup_ms=$(( $(node -p 'Date.now()') - start_ms ))
}

start_application
children=$(docker top "$application" -eo pid,args)
printf '%s\n' "$children" | grep 'dist-runtime/runtime/http.mjs' >/dev/null
worker_pid=$(docker exec "$application" sh -c \
  "ps -o pid,args | awk '/dist-runtime\\/runtime\\/[w]orker.mjs/ { print \$1; exit }'")
test -n "$worker_pid"
docker exec "$application" kill -9 "$worker_pid"
deadline=$(( $(date +%s) + 50 ))
while [ "$(docker inspect -f '{{.State.Running}}' "$application")" = true ]; do
  if [ "$(date +%s)" -ge "$deadline" ]; then echo 'container survived child failure' >&2; exit 1; fi
  sleep 1
done
test "$(docker inspect -f '{{.State.ExitCode}}' "$application")" -ne 0

start_application
redirect_headers=$(curl -sS -D - -o /dev/null "http://127.0.0.1:$host_port/org/example")
printf '%s\n' "$redirect_headers" | grep -E '^HTTP/[0-9.]+ 308 ' >/dev/null
printf '%s\n' "$redirect_headers" |
  grep -Ei '^location: (http://127\.0\.0\.1:3140)?/o/example?$' >/dev/null
memory=$(docker stats --no-stream --format '{{.MemUsage}}' "$application" | cut -d/ -f1 | xargs)
latency_ms=$(for _request in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
  curl -fsS -o /dev/null -w '%{time_total}\n' "http://127.0.0.1:$host_port/healthz"
done | awk '{ total += $1 } END { printf "%.3f", total * 1000 / NR }')
started=$(date +%s)
docker stop "$application" >/dev/null
elapsed=$(( $(date +%s) - started ))
test "$elapsed" -lt 60
test "$(docker inspect -f '{{.State.ExitCode}}' "$application")" -eq 0

sdk_count=$(docker run --rm "$image" sh -c "find node_modules/.pnpm -maxdepth 1 -type d -name '@dbos-inc+dbos-sdk@*' | wc -l")
test "$sdk_count" -eq 1
if docker run --rm "$image" sh -c "grep -R -l '/Users/ronitnath/dev/worktrees' dist-runtime package.json" | grep .; then
  echo 'runtime image contains a sibling-worktree path' >&2
  exit 1
fi
size=$(docker image inspect -f '{{.Size}}' "$image")
printf '{"ok":true,"imageBytes":%s,"startupMs":%s,"idleMemory":"%s","healthMeanMs":%s,"stopSeconds":%s}\n' \
  "$size" "$startup_ms" "$memory" "$latency_ms" "$elapsed"
