#!/usr/bin/env bash
# Size gate (procedures/engineering.md §Structure, docs/rebuild/plan.md).
#
#   composition roots (crates/server/src/main.rs, any lib.rs)  fail over 200
#   production files under crates/*/src                        warn over 350
#                                                              fail over 600
#
# A file over 600 lines is a design decision, not an accident: split it, or
# record the single-responsibility justification and raise the exclusion here
# by name. crates/starscape/legacy/ is the pre-rebuild renderer parked for the
# S1 worker to mine and delete — it is not in the build and not gated.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

warn_at=350
fail_at=600
root_fail_at=200

failures=0
warnings=0

while IFS= read -r file; do
  case "$file" in
    crates/starscape/legacy/*) continue ;;
  esac
  lines=$(wc -l < "$file" | tr -d ' ')

  case "$file" in
    crates/server/src/main.rs | */src/lib.rs)
      if [ "$lines" -gt "$root_fail_at" ]; then
        printf 'FAIL %5s lines  %s (composition root, limit %s)\n' \
          "$lines" "$file" "$root_fail_at"
        failures=$((failures + 1))
        continue
      fi
      ;;
  esac

  if [ "$lines" -gt "$fail_at" ]; then
    printf 'FAIL %5s lines  %s (limit %s)\n' "$lines" "$file" "$fail_at"
    failures=$((failures + 1))
  elif [ "$lines" -gt "$warn_at" ]; then
    printf 'warn %5s lines  %s (review at %s)\n' "$lines" "$file" "$warn_at"
    warnings=$((warnings + 1))
  fi
done < <(find crates -type f -name '*.rs' -path '*/src/*' | sort)

if [ "$failures" -gt 0 ]; then
  printf '\nsize gate: %s file(s) over limit\n' "$failures" >&2
  exit 1
fi

printf 'size gate: clean (%s warning(s))\n' "$warnings"
