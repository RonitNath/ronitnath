#!/usr/bin/env python3
"""Detail rows for the stars a catalogue fill added, and nothing else.

`sky_star_detail` is 3,087,894 rows and 257 MB compressed, and rebuilding it
to answer for two hundred new stars would be a day of Gaia TAP and an hour of
loading for a change of 0.006%. So this asks `build_detail.py`'s own routines
about exactly the Hipparcos numbers `build_bright.py` wrote to
`hip_filled.json` — HYG, the IAU name list, the constellation boundaries and
one small SIMBAD batch — and writes them out in the same two-column shape:

    data/sky_star_detail.delta.csv

Loaded with `node scripts/load-sky.mjs --delta <file>`, which upserts rather
than replacing the table. Everything upstream is pulled once, offline, here;
the site never contacts ESA or CDS.

    python3 tools/starcat/build_delta.py

Python 3 standard library only.
"""

from __future__ import annotations

import csv
import json
import os
from pathlib import Path

import build_detail as detail

HERE = Path(__file__).resolve().parent
OUT = detail.DATA / "sky_star_detail.delta.csv"


def filled_hips() -> list[str]:
    base = Path(os.environ.get("RN_BRIGHT_DIR", HERE / "data"))
    stars = json.loads((base / "hip_filled.json").read_text())["stars"]
    return [str(star["hip"]) for star in stars]


def simbad_for(hips: list[str], batch_size: int = 200) -> dict:
    """SIMBAD's rows for these stars, cached beside the full build's but not in
    it.

    `build_detail.py` caches its SIMBAD pull as `tap/simbad/batch-NNNN.csv` and
    skips any batch whose file already exists. A partial pull filed under those
    names would make a later full build skip a batch of five hundred stars it
    never fetched, so the delta gets its own directory and its own small query.
    """
    cache = detail.TAP_CACHE / "simbad-delta"
    cache.mkdir(parents=True, exist_ok=True)
    batches = [hips[at:at + batch_size] for at in range(0, len(hips), batch_size)]
    detail.log(f"simbad: {len(hips)} ids in {len(batches)} batches")
    merged: dict[str, dict] = {}
    for index, batch in enumerate(batches):
        path = cache / f"batch-{index:04}.csv"
        if not (path.exists() and path.stat().st_size > 20):
            quoted = ",".join("'HIP " + hip + "'" for hip in batch)
            body = detail.tap_sync(
                detail.SIMBAD_TAP,
                "SELECT i.id AS query_id, b.main_id, b.otype_txt, b.sp_type,"
                " b.plx_value, b.pmra, b.pmdec, b.rvz_radvel, a.id AS other_id"
                " FROM ident AS i"
                " JOIN basic AS b ON b.oid = i.oidref"
                " JOIN ident AS a ON a.oidref = i.oidref"
                f" WHERE i.id IN ({quoted})",
            )
            if b"query_id" not in body[:200].lower():
                raise RuntimeError(f"simbad batch {index}: {body[:200]!r}")
            path.write_bytes(body)
        with path.open(newline="", encoding="utf-8") as handle:
            for row in csv.DictReader(handle):
                entry = merged.setdefault(row["query_id"].strip(), {"ids": []})
                for field in ("main_id", "otype_txt", "sp_type", "plx_value",
                              "pmra", "pmdec", "rvz_radvel"):
                    value = (row.get(field) or "").strip()
                    if value and field not in entry:
                        entry[field] = value
                other = (row.get("other_id") or "").strip()
                if other and other not in entry["ids"]:
                    entry["ids"].append(other)
    return merged


def main() -> None:
    detail.TAP_CACHE.mkdir(parents=True, exist_ok=True)
    detail.stage_hyg()
    detail.stage_iau()
    detail.stage_bounds()

    hips = filled_hips()
    bounds = detail.load_boundaries(detail.TAP_CACHE / "constellation_boundaries.csv")
    hyg_by_hip, _ = detail.read_hyg(detail.TAP_CACHE / "hyg_v3.csv")
    iau_by_hip, iau_by_hd = detail.read_iau(detail.TAP_CACHE / "iau_csn.txt")
    simbad = simbad_for(hips)

    stats = {"withHyg": 0, "withIau": 0, "withSimbad": 0, "withConstellation": 0}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle)
        for hip in hips:
            payload = detail.hip_payload(
                hip, hyg_by_hip, iau_by_hip, iau_by_hd, simbad, bounds, stats
            )
            writer.writerow(
                [f"h{hip}", json.dumps(detail.prune(payload), separators=(",", ":"))]
            )
    detail.log(
        f"delta: {len(hips)} rows -> {OUT} ({OUT.stat().st_size:,} bytes); "
        f"{stats['withHyg']} with HYG, {stats['withIau']} named, "
        f"{stats['withSimbad']} with SIMBAD, {stats['withConstellation']} placed"
    )


if __name__ == "__main__":
    main()
