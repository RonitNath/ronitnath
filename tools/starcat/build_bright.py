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
- data/hip_6.5.csv — same TAP server, the whole naked-eye Hipparcos catalogue,
  cut at the same depth Gaia is, with the proper motions the epochs need:
    SELECT hip, ra, dec, hp_mag, b_v, plx, pm_ra, pm_de
    FROM public.hipparcos_newreduction WHERE hp_mag < 6.5

  Hipparcos positions are epoch 1991.25 and Gaia DR3's are 2016.0, so every
  Hipparcos row is carried forward 24.75 years by its own proper motion before
  it is either matched or stored. Epsilon Cygni moves 480 mas/yr: uncorrected
  it lands twelve arcseconds from its Gaia record, which is four times the
  cross-match radius, so the star is added a second time and drawn twice.

Merge, in two tiers, because "usable Gaia row" means two different things at
the two ends of a naked-eye catalogue.

Brighter than Hp 2.5, Gaia is not usable at all: DR3's window class saturates
above G ~ 1.7 and its photometry is unreliable well past that, so Hipparcos is
authoritative and a Gaia row matching one of those stars is dropped. That is
the original merge, unchanged, and it is why Sirius, Vega and Alioth are
Hipparcos records with Hipparcos magnitudes.

Fainter than that, Gaia is authoritative and a Hipparcos star is only *added*
where no Gaia row matches it. This is the half that was missing: the first
build cut the supplement at Hp < 2.5 and never looked below, so the stars
between the two cuts — Gaia unusable, Hipparcos absent — belonged to neither
half. Sheratan, Menkar, Mahasim, Gienah and Enif are all V 2.4 to 2.7 and were
drawn nowhere at all. Adding rather than replacing fills exactly that hole and
leaves every star Gaia does measure carrying Gaia's own BP-RP colour rather
than the cruder B-V ramp.

A Hipparcos star counts as matched when a Gaia row is within 3 arcsec *and*
within 1.5 magnitudes of it. With both sides at the same epoch, 3 arcsec is
catalogue error and the residual of a proper motion, not the motion itself.
The magnitude agreement is what keeps Gaia's *saturated* photometry from
counting as a measurement of the same star — Mahasim's Gaia row reads 7.28 for
a V 2.62 star, 4.7 magnitudes out, so it does not match and the Hipparcos row
is added. Where a star *is* added, any Gaia record within 3 arcsec of it goes,
magnitude or no magnitude: that record is the same star seen badly, and the
disagreement is why it was added.

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

`named.json` and `sky/lines.bin` address stars by *index* into this file, so
every record this build inserts moves the ones after it. Both are rebuilt by
their own builders — `build_names.py`, `build_lines.py` — and both write down
the content hash of the catalogue they resolved against. A test refuses a pair
whose hashes disagree, which is what stops a rebuilt catalogue from shipping
beside a name list that labels the wrong stars.
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
from crossmatch import DecIndex, unit  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
DATA = Path(__file__).resolve().parent / "data"
# The published catalogue is content-addressed, so this writes the bare name
# and `name_assets.py` stamps it afterwards.
OUT = ROOT / "public" / "stars" / "bright.bin"
# Which Hipparcos stars the fill added, so the builders downstream do not have
# to re-run the cross-match to find out: `build_star_lod.py` clears a radius
# around them in the deep tiles, `build_delta.py` builds the detail rows the
# panel answers with, and `build_detail.py` counts them among the HIP-keyed
# stars SIMBAD is asked about.
FILLED = DATA / "hip_filled.json"

# Where Gaia stops being a measurement and starts being a saturated pixel.
# Above this the Hipparcos row wins outright; below it Gaia does, and the
# Hipparcos row is only there to fill a gap.
BRIGHT_END_HP = 2.5
# Hipparcos is epoch 1991.25 and Gaia DR3 is 2016.0.
EPOCH_YEARS = 2016.0 - 1991.25

STRIDE = 32
KIND_GAIA = 0
KIND_HIP = 1


# Colour indices for rows whose photometry is missing: a G-type star, which is
# the commonest thing a naked-eye catalogue can be silent about.
DEFAULT_BP_RP = 0.82
DEFAULT_B_V = 0.65

RAMP = ramp()


def read_hipparcos() -> list[dict]:
    """Every naked-eye Hipparcos star, at Gaia's epoch rather than its own."""
    stars = []
    with open(DATA / "hip_6.5.csv") as handle:
        for row in csv.DictReader(handle):
            b_v = float(row["b_v"]) if row["b_v"] else DEFAULT_B_V
            dec = float(row["dec"])
            # Proper motion is mas/yr, and pm_ra is already the great-circle
            # rate (mu_alpha* = mu_alpha cos delta), so only the declination
            # scaling has to be undone to turn it back into degrees of RA.
            pm_ra = float(row["pm_ra"] or 0.0)
            pm_de = float(row["pm_de"] or 0.0)
            scale = EPOCH_YEARS / (3_600_000.0 * max(1e-6, math.cos(math.radians(dec))))
            stars.append(
                {
                    "vec": unit(
                        float(row["ra"]) + pm_ra * scale,
                        dec + pm_de * EPOCH_YEARS / 3_600_000.0,
                    ),
                    "mag": float(row["hp_mag"]),
                    "level": level_for(teff_from_b_v(b_v)),
                    "id": int(row["hip"]),
                    "kind": KIND_HIP,
                }
            )
    return stars


def read_gaia(bright_end: list[dict]) -> tuple[list[dict], int]:
    """Every Gaia row, minus the ones that are a saturated look at a star the
    Hipparcos bright end already carries."""
    index = DecIndex(bright_end)
    stars, dropped = [], 0
    with open(DATA / "gaia_g6.5.csv") as handle:
        for row in csv.DictReader(handle):
            vec = unit(float(row["ra"]), float(row["dec"]))
            mag = float(row["phot_g_mean_mag"])
            if index.matches(vec, mag):
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


def fill_from_hipparcos(
    gaia: list[dict], hip: list[dict]
) -> tuple[list[dict], list[dict]]:
    """The Hipparcos stars Gaia has no usable row for, and the Gaia rows that
    have to go with them.

    Every other Hipparcos row is discarded: Gaia measured that star better,
    and two records of one star would draw it twice.

    The drop in the other direction is the same rule read the other way round.
    Forty-seven of the filled stars *do* have a Gaia record within three
    arcseconds — a saturated one, which is why the magnitude test refused it
    and the star was filled at all. Left in place it draws the star a second
    time at a magnitude that is not its own: HIP 92862 at 3.93 with a Gaia
    ghost at 2.36 two arcseconds away.
    """
    index = DecIndex(gaia)
    added = [star for star in hip if not index.matches(star["vec"], star["mag"])]
    filled = DecIndex(added)
    kept = [star for star in gaia if not filled.matches(star["vec"], None)]
    return added, kept


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


def write_filled(added: list[dict]) -> None:
    """The added stars, for the builders that have to answer for them.

    Position and magnitude travel with the HIP number because the LOD dedupe
    needs a direction and nothing downstream should be re-reading a TAP CSV to
    get one.
    """
    FILLED.write_text(
        json.dumps(
            {
                "source": "tools/starcat/build_bright.py",
                "note": "Hipparcos stars added where Gaia DR3 has no usable row",
                "stars": [
                    {
                        "hip": star["id"],
                        "hpMag": round(star["mag"], 4),
                        "vec": [round(component, 9) for component in star["vec"]],
                    }
                    for star in sorted(added, key=lambda star: star["mag"])
                ],
            },
            indent=2,
        )
        + "\n"
    )


def main() -> None:
    hipparcos = read_hipparcos()
    bright_end = [star for star in hipparcos if star["mag"] < BRIGHT_END_HP]
    gaia, dropped = read_gaia(bright_end)
    before = len(gaia)
    added, gaia = fill_from_hipparcos(
        gaia, [star for star in hipparcos if star["mag"] >= BRIGHT_END_HP]
    )
    ghosts = before - len(gaia)
    stars = sorted(bright_end + gaia + added, key=lambda star: star["mag"])

    publish(OUT, pack(stars))
    write_filled(added)

    levels = sorted({star["level"] for star in stars})
    print(
        f"{len(stars)} stars ({len(gaia)} gaia, {len(bright_end)} hipparcos "
        f"bright end with {dropped} saturated gaia rows dropped, {len(added)} "
        f"hipparcos filling what Gaia has no usable row for and "
        f"{ghosts} more saturated gaia rows dropped under them) -> {OUT} "
        f"({OUT.stat().st_size / 1024:.0f} KiB)"
    )
    print(f"colour levels used: {len(levels)} of {len(RAMP)} ({levels[0]}..{levels[-1]})")
    print(f"filled stars listed in {FILLED.relative_to(ROOT)}")
    print("every record index moved: now run build_names.py, build_lines.py, "
          "build_star_lod.py --resort, name_assets.py")


if __name__ == "__main__":
    main()
