#!/bin/sh
set -eu

ESBUILD=${ESBUILD:-esbuild}
cd "$(dirname "$0")"

rm -rf ../static/dist
mkdir -p ../static/dist

"$ESBUILD" \
  src/entries/site.ts \
  src/entries/guestbook.ts \
  src/entries/event_rsvp.ts \
  src/entries/events_admin.ts \
  --outdir=../static/dist \
  --bundle \
  --format=esm \
  --splitting \
  --minify \
  --target=es2022 \
  '--entry-names=[name]' \
  '--chunk-names=chunks/[name]-[hash]' \
  '--asset-names=assets/[name]' \
  "$@"
