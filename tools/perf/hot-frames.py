#!/usr/bin/env python3
"""Read a samply profile and say which functions the samples actually landed in.

A flame graph is a picture and a perf report has to quote numbers, so this
walks the Firefox-profiler format samply writes and prints, per thread:

  self   the leaf of each sample — where the CPU was at that instant
  total  every function on the stack — where the time was spent *under*

`samply record --save-only` does not symbolicate: the profile carries library
offsets and leaves the names to the viewer's symbol server. So this does the
same job with the tools the machine already has — `atos` against the binary
samply recorded, then `rustfilt` — for frames in `rn-site`, and labels the rest
by the library they came from, which is all a report needs to say about
`libsystem_kernel`.

Usage:
  tools/perf/hot-frames.py <profile.json.gz> [--binary target/debug/rn-site]
                           [--top 20] [--thread NAME]
"""

import gzip
import json
import re
import shutil
import subprocess
import sys
from collections import Counter

OFFSET = re.compile(r"^0x[0-9a-f]+$")


def argument(name, fallback=None):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else fallback


def symbolicate(offsets, binary):
    """Offset string → demangled name, via atos and rustfilt."""
    if not offsets or not shutil.which("atos"):
        return {}
    try:
        atos = subprocess.run(
            ["atos", "-o", binary, "-arch", "arm64", "--offset"],
            input="\n".join(offsets),
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except (subprocess.CalledProcessError, OSError):
        return {}
    if shutil.which("rustfilt"):
        atos = subprocess.run(
            ["rustfilt"], input=atos, capture_output=True, text=True
        ).stdout
    names = {}
    for offset, line in zip(offsets, atos.splitlines()):
        line = re.sub(r"\s*\(in [^)]*\)( \+ \d+)?$", "", line.strip())
        names[offset] = line or offset
    return names


def labels(thread, binary):
    """funcIndex → a name worth printing."""
    strings = thread["stringArray"]
    funcs = thread["funcTable"]
    resources = thread["resourceTable"]

    raw, lib = [], []
    for index in range(funcs["length"]):
        raw.append(strings[funcs["name"][index]])
        resource = funcs["resource"][index]
        lib.append(
            strings[resources["name"][resource]]
            if resource is not None and resource >= 0
            else ""
        )

    binary_name = binary.rsplit("/", 1)[-1]
    wanted = sorted(
        {raw[i] for i in range(len(raw)) if OFFSET.match(raw[i]) and lib[i] == binary_name}
    )
    resolved = symbolicate(wanted, binary)

    out = []
    for index in range(len(raw)):
        name = raw[index]
        if OFFSET.match(name):
            name = resolved.get(name, f"{lib[index] or '?'}+{name}")
        out.append(name)
    return out


def hot(thread, names):
    frames = thread["frameTable"]["func"]
    prefix = thread["stackTable"]["prefix"]
    stack_frame = thread["stackTable"]["frame"]

    def name_of(stack):
        return names[frames[stack_frame[stack]]]

    selves, totals = Counter(), Counter()
    for stack in thread["samples"]["stack"]:
        if stack is None:
            continue
        selves[name_of(stack)] += 1
        seen, walk = set(), stack
        while walk is not None:
            seen.add(name_of(walk))
            walk = prefix[walk]
        for one in seen:
            totals[one] += 1
    return selves, totals


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    path = sys.argv[1]
    binary = argument("--binary", "target/debug/rn-site")
    top = int(argument("--top", 20))
    wanted = argument("--thread")

    with gzip.open(path) as handle:
        profile = json.load(handle)

    print(f"# {path}  (symbols from {binary})")
    threads = [
        thread
        for thread in profile["threads"]
        if thread["samples"]["length"] > 0
        and (wanted is None or thread["name"] == wanted)
    ]
    threads.sort(key=lambda thread: -thread["samples"]["length"])
    for thread in threads:
        names = labels(thread, binary)
        selves, totals = hot(thread, names)
        count = thread["samples"]["length"]
        print(f"\n## thread {thread['name']!r} — {count} samples")
        print("   self  function")
        for name, hits in selves.most_common(top):
            print(f"  {100 * hits / count:5.1f}%  {name}")
        print("  total  function")
        for name, hits in totals.most_common(top):
            print(f"  {100 * hits / count:5.1f}%  {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
