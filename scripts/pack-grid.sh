#!/bin/sh
set -eu

source_dir=${GRID_RUNTIME_SOURCE:-/Users/ronitnath/dev/worktrees/grid-dbos-runtime}
destination=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)/vendor

pnpm --dir "$source_dir" --filter @isoastra/fleet-events build
pnpm --dir "$source_dir" --filter @isoastra/fleet-workflows build
pnpm --dir "$source_dir" --filter @isoastra/fleet-runtime build
pnpm --dir "$source_dir" --filter @isoastra/fleet-events pack --pack-destination "$destination"
pnpm --dir "$source_dir" --filter @isoastra/fleet-workflows pack --pack-destination "$destination"
pnpm --dir "$source_dir" --filter @isoastra/fleet-runtime pack --pack-destination "$destination"
(cd "$destination" && shasum -a 256 ./*.tgz > SHA256SUMS)
