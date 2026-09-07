#!/usr/bin/env python3
"""Build public/stars/bright.bin (STR2) from Gaia DR3 + Hipparcos.

Carried from the pre-rebuild site's tools/starcat.py and extended: the wire
format now keeps each star's catalogue identity, and the display colour comes
from a temperature rather than from a hand-drawn ramp (colour.py).

Sources — snapshots committed under tools/starcat/data/, queries recorded here
so a refresh is reproducible. Nothing in the running site ever contacts ESA or
CDS; these files are pulled once, by hand, and checked in.

- data/gaia_g6.5.csv — ESA Gaia TAP sync query:
    SELECT source_id, ra, dec, phot_g_mean_mag, bp_rp, parallax
    FROM gaiadr3.gaia_source_lite WHERE phot_g_mean_mag < 6.5
- data/hip_2.5.csv — same TAP server, bright-end supplement (Gaia saturates
  above G ~ 1.7, so Sirius, Vega, Alpha Cen and the rest of the bright end are
  absent from DR3's usable range):
    SELECT hip, ra, dec, hp_mag, b_v, plx
    FROM public.hipparcos_newreduction WHERE hp_mag < 2.5

Merge: Hipparcos rows are authoritative for the bright end. A Gaia row is only
dropped when it is within 3 arcsec of a Hipparcos star *and* within 1.5 mag of
it. Hipparcos is epoch 1991.25 and Gaia DR3 is epoch 2016.0: 3 arcsec covers
100 mas/yr of proper motion over 24.75 years plus half an arcsec of catalogue
and rounding margin, and the magnitude agreement stops an exceptionally close
but clearly fainter neighbour from being erased.

Output, little-endian:
    magic  b"STR2"
    count  u32
    per star, 32 bytes, sorted brightest first so a prefix read is still the
    brightest sky rather than a random subset of it:
        x, y, z   f32   unit vector, equatorial (ICRS-ish; the epoch drift
                        between the 1991 and 2016 catalogues is far below a
                        pixel at this scale)
        mag       f32   G or Hp magnitude
        r,g,b,a   u8    display colour, quantised onto colour.ramp(); a = 255
        id        u64   Gaia DR3 source_id, or the Hipparcos number
        kind      u8    0 = Gaia DR3, 1 = Hipparcos
        pad       3     to a 32-byte stride

The record is 32 bytes, not the 28 the S1 brief names: 20 (STR1) + 8 (id) + 1
(kind) + 3 (pad) is 32, and the brief's own field list is what is built here.

named.json holds 50 stars by *index* into this file, so the last thing the
build does is re-derive those indices by matching each named star's direction
against the previous build, and refuse to publish if any of the 50 fails to
match. A catalogue whose named indices have quietly shifted labels the wrong
stars, which is worse than labelling none.
"""

from __future__ import annotations

import csv
import json
import math
import os
import struct
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from colour import level_for, ramp, teff_from_b_v, teff_from_bp_rp  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
DATA = Path(__file__).resolve().parent / "data"
OUT = ROOT / "public" / "stars" / "bright.bin"
NAMED = ROOT / "public" / "stars" / "named.json"

MATCH_RADIUS_DEG = 3.0 / 3_600.0
MATCH_MAG_DELTA = 1.5

STRIDE = 32
KIND_GAIA = 0
KIND_HIP = 1

# How far a named star is allowed to have moved between two builds before the
# match is refused. The direction is the same number in both files, so this is
# a float-rounding tolerance, not a search radius.
NAMED_MATCH_DEG = 1.0 / 3_600.0

# Colour indices for rows whose photometry is missing: a G-type star, which is
# the commonest thing a naked-eye catalogue can be silent about.
DEFAULT_BP_RP = 0.82
DEFAULT_B_V = 0.65

RAMP = ramp()


def unit(ra_deg: float, dec_deg: float) -> tuple[float, float, float]:
    ra, dec = math.radians(ra_deg), math.radians(dec_deg)
    return (
        math.cos(dec) * math.cos(ra),
        math.cos(dec) * math.sin(ra),
        math.sin(dec),
    )


def ang_sep_deg(a, b) -> float:
    """Angle between two directions, in degrees.

    Both sides are re-normalised and the angle comes from the chord rather
    than from acos(dot): the catalogue's directions are stored as f32, whose
    norm is only 1 to about six parts in a hundred million, and acos turns
    that into 70 arcsec of phantom separation — enough to make a named star
    fail to match itself between two builds.
    """

    def normalise(v):
        norm = math.sqrt(sum(c * c for c in v)) or 1.0
        return [c / norm for c in v]

    ua, ub = normalise(a), normalise(b)
    chord = math.sqrt(sum((x - y) ** 2 for x, y in zip(ua, ub)))
    return math.degrees(2.0 * math.asin(min(1.0, chord / 2.0)))


def read_hipparcos() -> list[dict]:
    stars = []
    with open(DATA / "hip_2.5.csv") as handle:
        for row in csv.DictReader(handle):
            b_v = float(row["b_v"]) if row["b_v"] else DEFAULT_B_V
            stars.append(
                {
                    "vec": unit(float(row["ra"]), float(row["dec"])),
                    "mag": float(row["hp_mag"]),
                    "level": level_for(teff_from_b_v(b_v)),
                    "id": int(row["hip"]),
                    "kind": KIND_HIP,
                }
            )
    return stars


def read_gaia(hip: list[dict]) -> tuple[list[dict], int]:
    stars, dropped = [], 0
    with open(DATA / "gaia_g6.5.csv") as handle:
        for row in csv.DictReader(handle):
            vec = unit(float(row["ra"]), float(row["dec"]))
            mag = float(row["phot_g_mean_mag"])
            if any(
                ang_sep_deg(vec, star["vec"]) < MATCH_RADIUS_DEG
                and abs(mag - star["mag"]) <= MATCH_MAG_DELTA
                for star in hip
            ):
                dropped += 1
                continue
            bp_rp = float(row["bp_rp"]) if row["bp_rp"] else DEFAULT_BP_RP
            stars.append(
                {
                    "vec": vec,
                    "mag": mag,
                    "level": level_for(teff_from_bp_rp(bp_rp)),
                    "id": int(row["source_id"]),
                    "kind": KIND_GAIA,
                }
            )
    return stars, dropped


def pack(stars: list[dict]) -> bytes:
    payload = bytearray(b"STR2")
    payload += struct.pack("<I", len(stars))
    for star in stars:
        x, y, z = star["vec"]
        r, g, b = RAMP[star["level"]]
        payload += struct.pack(
            "<ffffBBBBQBBBB",
            x,
            y,
            z,
            star["mag"],
            r,
            g,
            b,
            255,
            star["id"],
            star["kind"],
            0,
            0,
            0,
        )
    assert len(payload) == 8 + STRIDE * len(stars)
    return bytes(payload)


def previous_directions() -> list[tuple[float, float, float]]:
    """Every star direction in the file this build is about to replace.

    Reads STR1 or STR2, because the first STR2 build has an STR1 file to match
    against and every later one has an STR2 file.
    """
    if not OUT.exists():
        raise SystemExit(f"{OUT} is missing: nothing to re-derive named indices from")
    blob = OUT.read_bytes()
    magic, count = blob[:4], struct.unpack_from("<I", blob, 4)[0]
    stride = {b"STR1": 20, b"STR2": 32}.get(magic)
    if stride is None:
        raise SystemExit(f"{OUT} carries an unknown magic {magic!r}")
    if len(blob) != 8 + stride * count:
        raise SystemExit(f"{OUT} is truncated")
    return [struct.unpack_from("<fff", blob, 8 + index * stride) for index in range(count)]


def rederive_named(stars: list[dict]) -> tuple[dict, int]:
    """named.json with every brightIndex matched by position into `stars`."""
    named = json.loads(NAMED.read_text())
    before = previous_directions()
    # A flat scan per named star is 50 x 12,191 dot products, which is nothing
    # once a year; a spatial index here would be more code than it saves.
    moved = 0
    for entry in named["stars"]:
        old = entry["brightIndex"]
        if not 0 <= old < len(before):
            raise SystemExit(f"{entry['name']} points outside the previous catalogue")
        target = before[old]
        best, best_sep = -1, 180.0
        for index, star in enumerate(stars):
            sep = ang_sep_deg(target, star["vec"])
            if sep < best_sep:
                best, best_sep = index, sep
        if best_sep > NAMED_MATCH_DEG:
            raise SystemExit(
                f"{entry['name']} (index {old}) has no counterpart in the new "
                f"catalogue: nearest is {best_sep * 3600:.1f} arcsec away"
            )
        if best != old:
            moved += 1
        entry["brightIndex"] = best
    return named, moved


def render_named(named: dict) -> bytes:
    """named.json, one star per line — a diff between two builds should read
    as the handful of indices that moved, not as a reflowed file."""
    head = {key: value for key, value in named.items() if key != "stars"}
    lines = json.dumps(head, indent=2).rstrip("}").rstrip().rstrip(",")
    rows = ",\n".join(
        "    " + json.dumps(star, separators=(",", ":")) for star in named["stars"]
    )
    return f'{lines},\n  "stars": [\n{rows}\n  ]\n}}\n'.encode()


def publish(path: Path, payload: bytes) -> None:
    """Write only after the whole payload exists, and replace atomically."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as tmp:
        tmp.write(payload)
        staged = Path(tmp.name)
    staged.chmod(0o644)
    try:
        os.replace(staged, path)
    finally:
        staged.unlink(missing_ok=True)


def main() -> None:
    hip = read_hipparcos()
    gaia, dropped = read_gaia(hip)
    stars = sorted(hip + gaia, key=lambda star: star["mag"])

    named, moved = rederive_named(stars)
    publish(OUT, pack(stars))
    publish(NAMED, render_named(named))

    levels = sorted({star["level"] for star in stars})
    print(
        f"{len(stars)} stars ({len(hip)} hip + {len(gaia)} gaia, {dropped} gaia "
        f"dupes dropped) -> {OUT} ({OUT.stat().st_size / 1024:.0f} KiB)"
    )
    print(f"colour levels used: {len(levels)} of {len(RAMP)} ({levels[0]}..{levels[-1]})")
    print(f"named indices re-derived: 50 matched, {moved} moved")


if __name__ == "__main__":
    main()
