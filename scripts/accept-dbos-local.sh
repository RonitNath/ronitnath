#!/bin/sh
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
grid=${GRID_RUNTIME_SOURCE:-/Users/ronitnath/dev/worktrees/grid-notifications}

TEST_PG_BIN=${TEST_PG_BIN:-/opt/homebrew/opt/postgresql@16/bin} pnpm --dir "$grid" gate
TEST_PG_BIN=${TEST_PG_BIN:-/opt/homebrew/opt/postgresql@16/bin} \
  pnpm --dir "$grid" --filter @isoastra/fleet-workflows test:integration
GRID_RUNTIME_SOURCE="$grid" "$root/scripts/pack-grid.sh"
(cd "$root/vendor" && shasum -a 256 -c SHA256SUMS)
pnpm --dir "$root" install --frozen-lockfile
pnpm --dir "$root" gate
TEST_PG_BIN=${TEST_PG_BIN:-/opt/homebrew/opt/postgresql@16/bin} pnpm --dir "$root" test:runtime
TEST_PG_BIN=${TEST_PG_BIN:-/opt/homebrew/opt/postgresql@16/bin} pnpm --dir "$root" e2e:prepared
sh "$root/scripts/test-container-local.sh"
