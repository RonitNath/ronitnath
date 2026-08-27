#!/usr/bin/env bash
# Seed a dev deployment with its first platform operator.
#
# The two halves cannot run at once, and that is the whole shape of this
# script: hiqlite holds an exclusive lock on its data directory, so the server
# that answers `POST /auth/register` and the `bootstrap-operator` subcommand
# that writes `platform:* #operator @person` take turns over the same database.
#
#   1. start the server, wait for /readyz
#   2. register the operator through the real form (nothing here writes rows)
#   3. stop it, and wait for the lock to go
#   4. `rn-site bootstrap-operator <email>` — the kernel refuses a second one
#
# Dev only, and idempotent: an address that is already registered and a
# deployment that already has an operator are both "already seeded", not
# failures. Nothing here prints or stores a secret beyond the dev password it
# was given, which is the one it is meant to hand back.
#
#   tools/seed.sh                       # defaults below
#   RN_SEED_EMAIL=me@example.invalid tools/seed.sh
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

email=${RN_SEED_EMAIL:-operator@example.invalid}
display=${RN_SEED_DISPLAY:-Operator}
password=${RN_SEED_PASSWORD:-an obviously fake dev password}
addr=${RN_SITE__ADDR:-127.0.0.1:3004}
db_path=${RN_SITE__DB_PATH:-data/db.sqlite}
binary=${RN_SEED_BINARY:-}

export RN_SITE__MODE=dev
export RN_SITE__ADDR="$addr"
export RN_SITE__DB_PATH="$db_path"

if [ -z "$binary" ]; then
  cargo build -p rn-site
  binary="$root/target/debug/rn-site"
fi

mkdir -p "$(dirname "$db_path")"

pid=""
stop() {
  [ -n "$pid" ] || return 0
  kill "$pid" 2>/dev/null || true
  # Wait for the process to go: the next step needs the data directory's lock.
  for _ in $(seq 1 100); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
  pid=""
}
trap stop EXIT

"$binary" >/tmp/rn-seed-server.log 2>&1 &
pid=$!

ready=""
for _ in $(seq 1 300); do
  if curl -fsS "http://$addr/readyz" >/dev/null 2>&1; then ready=1; break; fi
  kill -0 "$pid" 2>/dev/null || break
  sleep 0.2
done
if [ -z "$ready" ]; then
  echo "seed: the dev server never became ready; see /tmp/rn-seed-server.log" >&2
  exit 1
fi

status=$(curl -sS -o /dev/null -w '%{http_code}' \
  -X POST "http://$addr/auth/register" \
  -H 'sec-fetch-site: same-origin' \
  -H 'content-type: application/x-www-form-urlencoded' \
  --data-urlencode "display_name=$display" \
  --data-urlencode "email=$email" \
  --data-urlencode "password=$password")
case "$status" in
  303) echo "seed: registered $email" ;;
  # The decline is uniform by design, and an address already taken is the only
  # way this form refuses a well-formed dev registration.
  403) echo "seed: $email was refused — already registered" ;;
  *) echo "seed: /auth/register answered $status" >&2; exit 1 ;;
esac

stop

if "$binary" bootstrap-operator "$email"; then
  echo "seed: $email holds platform:* #operator"
else
  echo "seed: this deployment already has a platform operator; left alone"
fi

echo "seed: sign in at http://$addr/auth as $email"
