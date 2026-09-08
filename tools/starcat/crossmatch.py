#!/usr/bin/env python3
"""Directions, angles and the nearest-neighbour index the catalogue builds on.

Split out of `build_bright.py` when that file reached its 400-line cap. What
lives here is the geometry every merge asks: where a catalogue row points,
how far apart two directions are, and whether some indexed star is the star in
hand. The merge policy — which catalogue wins where — stays in the builder.

Python 3 standard library only.
"""

from __future__ import annotations

import math

# How near two rows have to be to be one star, and how far their magnitudes
# may disagree while still being a measurement of it.
MATCH_RADIUS_DEG = 3.0 / 3_600.0
MATCH_MAG_DELTA = 1.5


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


class DecIndex:
    """Nearest-neighbour lookup on declination bands.

    Twelve thousand Gaia rows against eight thousand Hipparcos ones is a
    hundred million angle computations in Python, which is minutes for a
    question every candidate answers within a tenth of a degree. Banding by z
    — the unit vector's third component is the sine of declination — turns it
    into a scan of the two or three bands a 3 arcsec radius can reach.
    """

    BAND = 0.01  # ~0.6 degrees at the equator, far wider than the radius.

    def __init__(self, stars: list[dict]) -> None:
        self.bands: dict[int, list[dict]] = {}
        for star in stars:
            self.bands.setdefault(int(star["vec"][2] / self.BAND), []).append(star)

    def matches(self, vec, mag: float | None) -> bool:
        """Whether any indexed star is the same star as this one.

        `mag` of None asks the question by position alone. That is the right
        question in one direction only: once the build has decided Gaia has no
        usable row for a star, a Gaia record three arcseconds away is that
        star measured badly — Gaia reads 2.36 where Hipparcos reads 3.93 —
        and the magnitude test would only certify the disagreement.
        """
        band = int(vec[2] / self.BAND)
        for offset in (-1, 0, 1):
            for star in self.bands.get(band + offset, ()):
                if mag is not None and abs(mag - star["mag"]) > MATCH_MAG_DELTA:
                    continue
                if ang_sep_deg(vec, star["vec"]) < MATCH_RADIUS_DEG:
                    return True
        return False


