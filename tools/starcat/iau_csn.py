#!/usr/bin/env python3
"""The IAU Catalog of Star Names, read the way it is written.

S3 read this file with a regular expression over whitespace, which is wrong
about it in one specific way: the two name columns are fixed-width and may
contain spaces. Twenty-four of the 451 names are two words, so `Rigil
Kentaurus` arrived as the name `Rigil` with `Kentaurus` in the diacritics
column, and neither half is the star's name. The file's own header says the
columns, so the columns are what this reads.

    #Name/ASCII       Name/Diacritics   Designation  ID    ID    Con #  ...
    0                 18                36           49    55    61  65 70

Past column 70 every field is a single token — WDS_J, mag, band, HIP, HD, RA,
Dec, date, and an optional notes flag — so those are split on whitespace. The
component column (65) is *not*: Mebsuta leaves it blank, and a row split on
whitespace from there loses a field and silently shifts every column after it.

Licence: IAU, CC BY 4.0 — <https://www.iau.org/public/themes/naming_stars/>.
"""

import urllib.request
from dataclasses import dataclass
from pathlib import Path

URL = "https://www.pas.rochester.edu/~emamajek/WGSN/IAU-CSN.txt"
BLANK = {"_", "-", "", "999999"}


@dataclass(frozen=True)
class StarName:
    """One approved name and the identifiers it hangs on."""

    name: str
    diacritics: str | None
    designation: str | None
    bayer: str | None
    constellation: str | None
    component: str | None
    magnitude: float | None
    hip: str | None
    hd: str | None
    ra_deg: float | None
    dec_deg: float | None


def _text(value: str) -> str | None:
    value = value.strip()
    return None if value in BLANK else value


def _number(value: str) -> float | None:
    value = value.strip()
    if value in BLANK:
        return None
    try:
        return float(value)
    except ValueError:
        return None


def parse(path: Path) -> list[StarName]:
    """Every approved name in the catalogue, in file order."""
    names: list[StarName] = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if not line.strip() or line[0] in "#$":
            continue
        tail = line[70:].split()
        if len(tail) < 8:
            continue
        _wds, magnitude, _band, hip, hd, ra, dec, _date = tail[:8]
        name = line[0:18].strip()
        if not name:
            continue
        names.append(
            StarName(
                name=name,
                diacritics=_text(line[18:36]),
                designation=_text(line[36:49]),
                bayer=_text(line[49:55]),
                constellation=_text(line[61:65]),
                component=_text(line[65:70]),
                magnitude=_number(magnitude),
                hip=_text(hip),
                hd=_text(hd),
                ra_deg=_number(ra),
                dec_deg=_number(dec),
            )
        )
    if len(names) < 400:
        raise SystemExit(f"{path} yielded only {len(names)} names")
    return names


def fetch(path: Path) -> Path:
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(URL, timeout=120) as response:
            path.write_bytes(response.read())
    return path


if __name__ == "__main__":
    rows = parse(fetch(Path(__file__).resolve().parents[2] / "data/tap/iau_csn.txt"))
    two_word = [row.name for row in rows if " " in row.name]
    print(f"{len(rows)} names, {len(two_word)} of two words")
    print(next(row for row in rows if row.name == "Rigil Kentaurus"))
