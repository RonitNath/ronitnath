#!/usr/bin/env python3
"""Where a command's milliseconds went, from a node's own log.

`rn_kernel::cmd::run` emits one `command phases` line per fresh commit under
`RUST_LOG=rn_kernel::cmd=debug`: the microseconds spent on the idempotency
replay, on planning (the preconditions), on the raft transaction, on reading
the audit row back, and on notifying the feed. This reads those lines out of a
node log and reports each phase's p50/p99, so a commit-latency number becomes
an account of itself rather than one total.

    tools/perf/phases.py target/cluster/node1/rn-site.log
"""

import json
import sys

PHASES = ["replay_us", "plan_us", "commit_us", "read_back_us", "notify_us", "total_us"]


def percentile(values, fraction):
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, int(fraction * len(ordered)))
    return ordered[index]


def main(paths):
    samples = {phase: [] for phase in PHASES}
    counted = 0
    for path in paths:
        with open(path, "r", errors="replace") as handle:
            for line in handle:
                if "command phases" not in line:
                    continue
                # The server logs JSON; a line that is not is not one of ours.
                try:
                    fields = json.loads(line)
                except ValueError:
                    continue
                if "total_us" not in fields:
                    continue
                counted += 1
                for phase in PHASES:
                    if phase in fields:
                        samples[phase].append(int(fields[phase]))
    if not counted:
        print("no `command phases` lines — was RUST_LOG=rn_kernel::cmd=debug set?")
        return 1
    print(f"{counted} commands")
    print(f"{'phase':<14}{'p50 ms':>10}{'p90 ms':>10}{'p99 ms':>10}{'max ms':>10}")
    for phase in PHASES:
        values = samples[phase]
        if not values:
            continue
        print(
            f"{phase[:-3]:<14}"
            f"{percentile(values, 0.50) / 1000:>10.3f}"
            f"{percentile(values, 0.90) / 1000:>10.3f}"
            f"{percentile(values, 0.99) / 1000:>10.3f}"
            f"{max(values) / 1000:>10.3f}"
        )
    return 0


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1:]))
