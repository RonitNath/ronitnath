#!/usr/bin/env python3
"""Grow `public/stars/named.json` to every IAU name the sky can actually draw.

S1 shipped 50 names, written by hand, because nothing on the page could say
which star an index was. S3 made every record addressable and gave the panel a
local dataset, so the constraint is gone: what ships now is the whole IAU
Catalog of Star Names, minus the ones whose star is not in `bright.bin` —
about a hundred of the 451, which are either fainter than the catalogue's
G <= 6.5 cut or among the handful of naked-eye stars Gaia saturates on.

The join is positional against the catalogue's own directions, the same way
`build_bright.py` re-derives indices between builds: the IAU file carries a
J2000 position per name, and 25 arcseconds is far tighter than the separation
between any two naked-eye stars.

Constellation and classification come from the same route the S3 panel takes —
the IAU row's own three-letter code expanded through `constellations.ts`, and
HYG v3's spectral type turned into the phrase the callout says. Distance is
HYG's parallax distance in light years, rounded the way the hand-written fifty
were rounded.

The fifty hand-checked entries keep their text. They were read against SIMBAD
one at a time, and where the generated phrase differs it differs in wording
("triple-star system" against "G-type main-sequence star" for Alpha Centauri),
not in fact; a build should not quietly overwrite a person's reading.

    python3 tools/starcat/build_names.py
"""

import argparse
import csv
import json
import math
import re
import struct
from pathlib import Path

import iau_csn

ROOT = Path(__file__).resolve().parent.parent.parent
IAU = ROOT / "data" / "tap" / "iau_csn.txt"
HYG = ROOT / "data" / "tap" / "hyg_v3.csv"
BRIGHT = ROOT / "public" / "stars" / "bright.bin"
NAMED = ROOT / "public" / "stars" / "named.json"
# The fifty S1 wrote by hand, kept as their own input: if the build read them
# back out of its own output it would treat last night's generated phrase as a
# person's reading, and the distinction would be gone after one run.
HAND = Path(__file__).resolve().parent / "data" / "named_hand.json"
CONSTELLATIONS_TS = ROOT / "src" / "features" / "sky" / "constellations.ts"

HEADER_LEN, STRIDE = 8, 32
# Arcturus moves two arcseconds a year, so its J2000 position in the IAU file
# and its Gaia epoch-2016 position in bright.bin are half an arcminute apart.
# Two arcminutes admits that; the magnitude test is what keeps the widened
# radius from finding a fainter neighbour instead.
MATCH_DEG = 120.0 / 3600.0
MAG_TOLERANCE = 2.5
PARSEC_LY = 3.261564
# HYG parks a star with no usable parallax at 100,000 parsecs. A distance the
# panel would print as "326,000 light years" for a naked-eye star is not a
# distance, so those entries are dropped rather than published.
HYG_UNKNOWN_PC = 99_000.0

SOURCES = [
    "IAU Catalog of Star Names",
    "HYG v3 (Astronomy Nexus)",
    "Hipparcos catalogue",
    "Gaia DR3",
]

# Spectral class -> the colour word the callout uses for a star that is not on
# the main sequence. On the main sequence the class letter is said instead:
# "A-type main-sequence star" is what the fifty hand-written entries say, and
# "white main-sequence star" would be a different claim about the same star.
COLOUR = {
    "O": "blue", "B": "blue", "A": "white", "F": "yellow-white",
    "G": "yellow", "K": "orange", "M": "red", "C": "red", "S": "red",
}
# Luminosity class -> the noun. Roman numerals, longest first, because "III"
# starts with "II" which starts with "I".
LUMINOSITY = [
    ("VIII", None), ("VII", "white dwarf"), ("VI", "subdwarf"),
    ("IV", "subgiant"), ("III", "giant"), ("II", "bright giant"),
    ("IB", "supergiant"), ("IAB", "supergiant"), ("IA", "supergiant"),
    ("I", "supergiant"), ("V", None),
]
SPECTRAL = re.compile(r"^([OBAFGKMCSDW])\s*[0-9.]*(.*)$")


def unit(ra_deg: float, dec_deg: float) -> tuple[float, float, float]:
    ra, dec = math.radians(ra_deg), math.radians(dec_deg)
    return math.cos(dec) * math.cos(ra), math.cos(dec) * math.sin(ra), math.sin(dec)


def read_bright() -> tuple[list[tuple[float, float, float]], list[float]]:
    blob = BRIGHT.read_bytes()
    if blob[:4] != b"STR2":
        raise SystemExit(f"{BRIGHT} is not an STR2 catalogue")
    count = struct.unpack_from("<I", blob, 4)[0]
    records = [
        struct.unpack_from("<ffff", blob, HEADER_LEN + index * STRIDE)
        for index in range(count)
    ]
    return [record[:3] for record in records], [record[3] for record in records]


def read_constellations() -> dict[str, str]:
    """The abbreviation/name table the panel already uses, read from its own
    module so the two can never drift apart."""
    source = CONSTELLATIONS_TS.read_text(encoding="utf-8")
    pairs = dict(re.findall(r"^\s{2}(\w{3}): '([^']+)',$", source, re.MULTILINE))
    if len(pairs) != 88:
        raise SystemExit(f"{CONSTELLATIONS_TS} yielded {len(pairs)} constellations")
    return pairs


def read_hyg() -> dict[str, dict]:
    """HYG rows keyed by HIP: spectral type and parallax distance."""
    rows: dict[str, dict] = {}
    with HYG.open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            if row.get("hip"):
                rows[str(int(float(row["hip"])))] = row
    return rows


def classify(spectral: str | None) -> str | None:
    """A spectral type as the phrase a callout says.

    `A1V` is "A-type main-sequence star"; `K1.5III` is "orange giant"; `B8Ia`
    is "blue supergiant". The colour word is only used off the main sequence,
    which is where a reader is being told something the class letter does not
    already say.
    """
    if not spectral:
        return None
    match = SPECTRAL.match(spectral.strip().upper())
    if not match:
        return None
    letter, tail = match.groups()
    if letter in {"D", "W"}:
        return "white dwarf" if letter == "D" else "Wolf-Rayet star"
    # HYG writes uncertainty, composites and variability into the type:
    # Polaris is "F7:Ib-IIv SB". Everything that is not a luminosity numeral
    # is dropped, and what is left is read from its first character.
    suffix = re.sub(r"[^IVAB]", "", tail.upper().split()[0] if tail.split() else "")
    for numeral, noun in LUMINOSITY:
        if suffix.startswith(numeral):
            if noun is None:
                break
            colour = COLOUR.get(letter)
            return f"{colour} {noun}" if colour else noun
    return f"{letter}-type main-sequence star"


def light_years(row: dict) -> float | None:
    """HYG's parallax distance in light years, rounded the way the fifty were:
    a tenth under 100 ly, ten light years under 1,000, a hundred beyond."""
    try:
        parsecs = float(row.get("dist") or 0)
    except ValueError:
        return None
    if parsecs <= 0 or parsecs >= HYG_UNKNOWN_PC:
        return None
    distance = parsecs * PARSEC_LY
    if distance < 100:
        return round(distance, 1)
    if distance < 1000:
        return float(round(distance, -1))
    return float(round(distance, -2))


def nearest(directions, magnitudes, target, magnitude) -> tuple[int, float]:
    """The record this name belongs to: nearest, then brightness.

    Alpha Centauri is why the second key exists. Its two components are five
    arcseconds apart and the pair moves three and a half arcseconds a year, so
    at the epoch difference between this file and the catalogue *both* records
    sit within half an arcminute of the IAU position and the nearer one is the
    fainter component. Magnitude separates them without ambiguity: A is 1.3
    magnitudes brighter than B, and the IAU row for Rigil Kentaurus says A's.
    So candidates are bucketed by separation to two arcminutes and the one
    whose brightness matches decides inside the bucket.
    """
    candidates = []
    for index, star in enumerate(directions):
        if magnitude is not None and abs(magnitudes[index] - magnitude) > MAG_TOLERANCE:
            continue
        dot = target[0] * star[0] + target[1] * star[1] + target[2] * star[2]
        separation = math.degrees(math.acos(max(-1.0, min(1.0, dot))))
        if separation <= MATCH_DEG:
            offset = abs(magnitudes[index] - magnitude) if magnitude is not None else 0.0
            candidates.append((offset, separation, index))
    if not candidates:
        return -1, 180.0
    offset, separation, index = min(candidates)
    return index, separation


def build() -> tuple[dict, dict[str, int]]:
    iau_csn.fetch(IAU)
    directions, magnitudes = read_bright()
    constellations = read_constellations()
    hyg = read_hyg()
    kept = {entry["name"]: entry for entry in json.loads(HAND.read_text())["stars"]}

    stars, taken = [], {}
    tally = {"names": 0, "unmatched": 0, "noDistance": 0, "duplicate": 0,
             "handKept": 0, "handOnly": 0}
    for row in iau_csn.parse(IAU):
        if row.ra_deg is None or row.dec_deg is None:
            tally["unmatched"] += 1
            continue
        index, separation = nearest(
            directions, magnitudes, unit(row.ra_deg, row.dec_deg), row.magnitude
        )
        if separation > MATCH_DEG:
            tally["unmatched"] += 1
            continue
        # Alpha Centauri A and B are one record at this resolution; the first
        # name wins, which is the brighter component the catalogue is really of.
        if index in taken:
            tally["duplicate"] += 1
            continue

        hand = kept.get(row.name)
        hyg_row = hyg.get(row.hip or "") or {}
        distance = hand["distanceLy"] if hand else light_years(hyg_row)
        constellation = (
            hand["constellation"] if hand else constellations.get(row.constellation or "")
        )
        classification = hand["classification"] if hand else classify(hyg_row.get("spect"))
        if not (distance and constellation and classification):
            tally["noDistance"] += 1
            continue
        taken[index] = row.name
        if hand:
            tally["handKept"] += 1
        stars.append({
            "brightIndex": index,
            "name": row.name,
            "constellation": constellation,
            "classification": classification,
            "distanceLy": distance,
        })

    # A hand-written entry the IAU list does not carry — "Alpha Centauri",
    # "Delta Velorum", "R Doradus", "Regor" — stays, unless its record has
    # already been claimed by the IAU name for the same star.
    for name, entry in kept.items():
        if name in taken.values() or entry["brightIndex"] in taken:
            continue
        taken[entry["brightIndex"]] = name
        tally["handOnly"] += 1
        stars.append(dict(entry))

    stars.sort(key=lambda star: star["brightIndex"])
    tally["names"] = len(stars)
    return {"version": 2, "sources": SOURCES, "stars": stars}, tally


def render(named: dict) -> str:
    """One star per line, so a diff between two builds reads as a list of
    stars rather than as one reflowed blob."""
    head = {key: value for key, value in named.items() if key != "stars"}
    body = ",\n".join(
        "    " + json.dumps(star, separators=(",", ":")) for star in named["stars"]
    )
    return json.dumps(head, indent=2)[:-2] + ',\n  "stars": [\n' + body + "\n  ]\n}\n"


def main() -> None:
    parser = argparse.ArgumentParser(description="build named.json from the IAU list")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    named, tally = build()
    superseded = [
        star["name"] for star in json.loads(NAMED.read_text())["stars"]
        if star["name"] not in {entry["name"] for entry in named["stars"]}
    ]
    if not args.dry_run:
        NAMED.write_text(render(named))
    print(
        f"{tally['names']} names ({tally['handKept']} hand-written kept), "
        f"{tally['unmatched']} not in bright.bin, {tally['duplicate']} same record, "
        f"{tally['noDistance']} without a distance or classification; "
        f"{tally['handOnly']} hand-only kept"
    )
    if superseded:
        print(f"superseded by their IAU name: {superseded}")


if __name__ == "__main__":
    main()
