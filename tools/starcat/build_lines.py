#!/usr/bin/env python3
"""Pack the constellation figures into indices the sky already has.

Stellarium's `constellationship.fab` draws each of the 88 figures as a run of
Hipparcos pairs: `Ori 15  HIP HIP  HIP HIP ...`. The browser has no Hipparcos
catalogue — it has `bright.bin`, 12,191 records addressed by their own index —
so the join happens here, once, and what ships is a list of index pairs.

The join is positional, for the same reason S3's Gaia/HYG join is: the bright
catalogue is keyed by Gaia `source_id` for all but 89 of its records, and Gaia
publishes no Hipparcos cross-match in `gaia_source`. HYG v3 carries a HIP number
and a J2000 position for every Hipparcos star, so a figure's HIP becomes a
direction, and the direction becomes the nearest bright record within a
tolerance wide enough for sixteen years of proper motion and narrow enough that
it cannot pick up a neighbour.

A pair that does not resolve at both ends is dropped rather than guessed: a line
to the wrong star is a wrong constellation, which is worse than a gap. The count
of dropped pairs is printed and recorded in `public/sky/lines.json`.

Output `public/sky/lines.bin`:

    b"CLIN"  u32 pair_count  (u16 a, u16 b) * pair_count

Sources
  Stellarium 23.4 `skycultures/modern/constellationship.fab` —
  CC BY-SA 4.0 + Free Art License (`skycultures/modern/info.ini`).
  HYG v3 (Astronomy Nexus, David Nash) — CC BY-SA 4.0, for HIP positions.
"""

import argparse
import csv
import json
import math
import struct
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
FAB = ROOT / "data" / "lines" / "constellationship.fab"
HYG = ROOT / "data" / "tap" / "hyg_v3.csv"
BRIGHT = ROOT / "public" / "stars" / "bright.bin"
OUT = ROOT / "public" / "sky" / "lines.bin"
MANIFEST = ROOT / "public" / "sky" / "lines.json"

FAB_URL = (
    "https://raw.githubusercontent.com/Stellarium/stellarium/v23.4/"
    "skycultures/modern/constellationship.fab"
)
HYG_URL = (
    "https://raw.githubusercontent.com/astronexus/HYG-Database/main/"
    "hyg/v3/hyg_v38.csv.gz"
)

HEADER_LEN = 8
STRIDE = 32
MAGIC = b"STR2"

# Sixteen years of proper motion moves a fast naked-eye star by about a minute
# of arc — beta Hydri, the fastest a figure is drawn to, by 68 arcseconds — and
# HYG's own positions add a little. Ninety arcseconds admits that drift; the
# magnitude test below is what keeps a widened radius from finding a neighbour,
# and for a naked-eye star the nearest catalogued neighbour is degrees away.
RADIUS_DEG = 90.0 / 3600.0
# Where the magnitude test refuses every candidate — Mira runs from 3rd to 10th
# magnitude, so HYG's single number is wrong for it most of the year — position
# alone decides, at the radius a bare positional match is safe at.
STRICT_RADIUS_DEG = 60.0 / 3600.0
# HYG's V against the catalogue's G: the two bands differ by up to a magnitude
# on a red star, and this only has to reject a coincidence, not grade a match.
MAG_TOLERANCE = 2.5
CELL_DEG = 0.5


def unit(ra_deg: float, dec_deg: float) -> tuple[float, float, float]:
    ra, dec = math.radians(ra_deg), math.radians(dec_deg)
    return math.cos(dec) * math.cos(ra), math.cos(dec) * math.sin(ra), math.sin(dec)


def read_bright() -> tuple[list[tuple[float, float, float]], list[float]]:
    """The bright catalogue's directions and magnitudes, by record index."""
    blob = BRIGHT.read_bytes()
    if blob[:4] != MAGIC:
        raise SystemExit(f"{BRIGHT} is not an STR2 catalogue")
    count = struct.unpack_from("<I", blob, 4)[0]
    if len(blob) != HEADER_LEN + count * STRIDE:
        raise SystemExit(f"{BRIGHT} is truncated")
    if count > 0xFFFF:
        raise SystemExit("the packed format addresses records with a u16")
    directions, magnitudes = [], []
    for index in range(count):
        at = HEADER_LEN + index * STRIDE
        x, y, z, mag = struct.unpack_from("<ffff", blob, at)
        directions.append((x, y, z))
        magnitudes.append(mag)
    return directions, magnitudes


def read_hip_positions() -> dict[int, tuple[tuple[float, float, float], float | None]]:
    """HIP number -> (J2000 unit vector, V magnitude), from HYG v3."""
    positions: dict[int, tuple[tuple[float, float, float], float | None]] = {}
    with HYG.open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            if not row.get("hip"):
                continue
            try:
                # HYG stores RA in hours.
                magnitude = float(row["mag"]) if row.get("mag") else None
                positions[int(float(row["hip"]))] = (
                    unit(float(row["ra"]) * 15.0, float(row["dec"])),
                    magnitude,
                )
            except (TypeError, ValueError):
                continue
    return positions


class SkyIndex:
    """Nearest bright record to a direction, on a coarse declination grid."""

    def __init__(self, directions, magnitudes):
        self.directions = directions
        self.magnitudes = magnitudes
        self.cells: dict[tuple[int, int], list[int]] = {}
        for index, (x, y, z) in enumerate(directions):
            self.cells.setdefault(self._cell(x, y, z), []).append(index)

    @staticmethod
    def _cell(x: float, y: float, z: float) -> tuple[int, int]:
        dec = math.degrees(math.asin(max(-1.0, min(1.0, z))))
        ra = math.degrees(math.atan2(y, x)) % 360.0
        return int(ra / CELL_DEG), int((dec + 90.0) / CELL_DEG)

    def nearest(self, direction, mag_hint: float | None,
                radius_deg: float = RADIUS_DEG) -> int | None:
        x, y, z = direction
        cx, cy = self._cell(x, y, z)
        # A cell is 0.5 deg of declination but shrinks in RA toward the poles,
        # so the RA span searched grows with 1/cos(dec) to stay a fixed angle.
        dec = math.degrees(math.asin(max(-1.0, min(1.0, z))))
        span = min(360, int(1 / max(1e-3, math.cos(math.radians(dec)))) + 1)
        limit = math.cos(math.radians(radius_deg))
        best, best_dot = None, limit
        ra_cells = int(360 / CELL_DEG)
        for dx in range(-span, span + 1):
            for dy in (-1, 0, 1):
                for index in self.cells.get(((cx + dx) % ra_cells, cy + dy), ()):
                    if mag_hint is not None and abs(self.magnitudes[index] - mag_hint) > MAG_TOLERANCE:
                        continue
                    bx, by, bz = self.directions[index]
                    dot = x * bx + y * by + z * bz
                    if dot > best_dot:
                        best, best_dot = index, dot
        return best


def read_figures(path: Path) -> list[tuple[str, list[tuple[int, int]]]]:
    """`constellationship.fab` -> per constellation, its HIP pairs."""
    figures = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        fields = line.split()
        abbreviation, segments = fields[0], int(fields[1])
        numbers = [int(value) for value in fields[2:]]
        if len(numbers) != segments * 2:
            raise SystemExit(f"{abbreviation}: {segments} segments, {len(numbers)} ids")
        figures.append(
            (abbreviation, [(numbers[i], numbers[i + 1]) for i in range(0, len(numbers), 2)])
        )
    return figures


def fetch(url: str, path: Path) -> None:
    if path.exists():
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    print(f"fetching {url}")
    with urllib.request.urlopen(url, timeout=120) as response:
        path.write_bytes(response.read())


def build() -> dict:
    fetch(FAB_URL, FAB)
    if not HYG.exists():
        raise SystemExit(
            f"{HYG} is missing; run tools/starcat/build_detail.py --stage hyg"
        )
    directions, magnitudes = read_bright()
    index = SkyIndex(directions, magnitudes)
    hip_positions = read_hip_positions()

    resolved: dict[int, int] = {}
    unresolved: set[int] = set()

    def resolve(hip: int) -> int | None:
        if hip in resolved:
            return resolved[hip]
        if hip in unresolved:
            return None
        entry = hip_positions.get(hip)
        if entry is None:
            unresolved.add(hip)
            return None
        found = index.nearest(entry[0], entry[1])
        if found is None:
            found = index.nearest(entry[0], None, STRICT_RADIUS_DEG)
        if found is None:
            unresolved.add(hip)
            return None
        resolved[hip] = found
        return found

    pairs: list[tuple[int, int]] = []
    dropped: list[tuple[str, int, int]] = []
    for abbreviation, figure in read_figures(FAB):
        for a_hip, b_hip in figure:
            a, b = resolve(a_hip), resolve(b_hip)
            if a is None or b is None or a == b:
                dropped.append((abbreviation, a_hip, b_hip))
                continue
            pairs.append((a, b))

    # Two constellations that share a star draw the same segment twice; the GL
    # pass would then blend it twice and one line would be brighter than its
    # neighbours for no reason a viewer could name.
    seen, unique = set(), []
    for a, b in pairs:
        key = (min(a, b), max(a, b))
        if key in seen:
            continue
        seen.add(key)
        unique.append(key)

    blob = bytearray(b"CLIN")
    blob += struct.pack("<I", len(unique))
    for a, b in unique:
        blob += struct.pack("<HH", a, b)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(bytes(blob))

    return {
        "source": FAB_URL,
        "licence": "CC BY-SA 4.0 + Free Art License (Stellarium modern skyculture)",
        "resolvedVia": "HYG v3 HIP positions, matched into bright.bin within 60 arcsec",
        "constellations": len(read_figures(FAB)),
        "pairs": len(unique),
        "duplicatePairs": len(pairs) - len(unique),
        "droppedPairs": len(dropped),
        "unresolvedHip": sorted(unresolved),
        "bytes": len(blob),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="pack constellation lines")
    parser.add_argument("--report", action="store_true", help="list dropped pairs")
    args = parser.parse_args()
    manifest = build()
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")
    print(
        f"{manifest['pairs']:,} pairs from {manifest['constellations']} figures; "
        f"{manifest['droppedPairs']} dropped, "
        f"{manifest['duplicatePairs']} duplicates removed, "
        f"{len(manifest['unresolvedHip'])} HIP unresolved "
        f"-> {OUT} ({manifest['bytes']:,} B)"
    )
    if args.report:
        print("unresolved HIP:", manifest["unresolvedHip"])


if __name__ == "__main__":
    main()
