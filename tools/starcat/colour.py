#!/usr/bin/env python3
"""Star colour, by way of a temperature rather than by taste.

The pre-rebuild builder interpolated five hand-picked RGB anchors against
BP-RP. That reads well but it is a drawing of star colour, not a measurement
of it, and once the renderer became physical (linear brightness accumulated
into an HDR buffer, then tone mapped) the colours had to be physical too or
the wings of a bright star would carry a hue nothing in the sky has.

So the route is now colour index -> effective temperature -> blackbody
spectrum -> CIE XYZ -> sRGB:

- BP-RP -> Teff: Mucciarelli & Bellazzini 2020, RNAAS 4, 52, the infrared
  flux method colour-Teff fit for Gaia DR2 photometry,
      theta = 5040/Teff = b0 + b1 C + b2 C^2
                          + b3 [Fe/H] + b4 [Fe/H]^2 + b5 [Fe/H] C
  with C = (BP-RP)_0. The catalogue carries no metallicity, so the three
  [Fe/H] terms drop and the dwarf coefficients (b0, b1, b2) =
  (0.4988, 0.4925, -0.0287) are what is left. Fitted over C in [0.38, 1.51]
  with 61 K residual scatter; a naked-eye catalogue runs a little past both
  ends and the quadratic stays monotonic and physical there (C = -0.4 gives
  16000 K, C = 3.0 gives 2900 K), which is all a colour ramp needs of it.
  The dwarf fit rather than the giant one because dwarfs are the bulk of the
  12,191 rows, and the two differ by under 10 K anywhere near solar colour.
- B-V -> Teff for the Hipparcos rows: Ballesteros 2012, EPL 97, 34008,
      Teff = 4600 K * (1/(0.92 (B-V) + 1.70) + 1/(0.92 (B-V) + 0.62))
  a two-blackbody fit that agrees with the Gaia route to a few per cent over
  the range the bright end occupies. Both catalogues then share one colour
  function, which is what stops the Hipparcos stars reading as a different
  species from their Gaia neighbours.
- Teff -> XYZ: Planck's law integrated against the CIE 1931 2-degree colour
  matching functions, using the multi-lobe Gaussian fits of Wyman, Sloan &
  Shirley 2013, JCGT 2(2), so no CMF table has to ship with the builder.
- XYZ -> sRGB: the sRGB primaries and transfer function (IEC 61966-2-1).

Two deliberate departures from photometric truth, both because the target is
a near-black page rather than a calibrated print:

- the result is normalised so the largest channel is 1. A star's *brightness*
  is the renderer's job (magnitude -> linear flux -> tone curve); the catalogue
  carries only its hue, and a 3000 K star that came out at a tenth the level of
  a 9000 K one would arrive on screen as a dim smudge with no colour left.
- a saturation step. Normalising the brightest channel to 1 is itself a
  desaturation — it lifts the two weaker channels — and it leaves a 15000 K
  star at (0.71, 0.78, 1.00), which on a near-black page reads as white. The
  chroma is pushed back out around the colour's own mean and then a small lift
  toward white keeps the reddest stars from going to a saturated orange no
  eye reports seeing. Together these land the ends within a couple of levels
  of the hand-drawn ramp the site shipped, which is the look being kept.
"""

from __future__ import annotations

import math

# The ramp's ends: the temperatures the shipped catalogue actually reaches,
# which are where the BP-RP relation is clamped (C = 3.0 and C = -0.4). Wider
# ends would spend levels on stars that are not in the file and quantise the
# ones that are more coarsely.
TEFF_MIN = 2_900.0
TEFF_MAX = 17_000.0

# How many colours the catalogue is allowed. Star colour is a one-dimensional
# family and a point two pixels across shows very little of it; 24 steps is
# below what the eye separates on a star, which is what stops the sky banding
# into visible colour tiers.
RAMP_LEVELS = 24

# Chroma restored after the max-channel normalisation, and then how far the
# result is pushed back toward white. See the module docstring.
SATURATION = 1.4
WHITE_LIFT = 0.12


# Mucciarelli & Bellazzini 2020, Table 1, (BP-RP) dwarfs, at [Fe/H] = 0.
_MB20_BP_RP = (0.4988, 0.4925, -0.0287)


def teff_from_bp_rp(bp_rp: float) -> float:
    """Mucciarelli & Bellazzini 2020 (BP-RP) -> Teff, solar metallicity."""
    c = max(-0.4, min(3.0, bp_rp))
    b0, b1, b2 = _MB20_BP_RP
    theta = b0 + b1 * c + b2 * c * c
    return 5_040.0 / theta


def teff_from_b_v(b_v: float) -> float:
    """Ballesteros 2012 B-V -> Teff, for the Hipparcos bright end."""
    x = max(-0.35, min(2.0, b_v))
    return 4_600.0 * (1.0 / (0.92 * x + 1.70) + 1.0 / (0.92 * x + 0.62))


def _gaussian(x: float, alpha: float, mu: float, s1: float, s2: float) -> float:
    """One piecewise-Gaussian lobe of a Wyman/Sloan/Shirley CMF fit."""
    t = (x - mu) * (1.0 / s1 if x < mu else 1.0 / s2)
    return alpha * math.exp(-0.5 * t * t)


def cie_xyz_at(wavelength_nm: float) -> tuple[float, float, float]:
    """The CIE 1931 2-degree observer, multi-lobe Gaussian fit (JCGT 2013)."""
    w = wavelength_nm
    x = (
        _gaussian(w, 1.056, 599.8, 37.9, 31.0)
        + _gaussian(w, 0.362, 442.0, 16.0, 26.7)
        + _gaussian(w, -0.065, 501.1, 20.4, 26.2)
    )
    y = _gaussian(w, 0.821, 568.8, 46.9, 40.5) + _gaussian(w, 0.286, 530.9, 16.3, 31.1)
    z = _gaussian(w, 1.217, 437.0, 11.8, 36.0) + _gaussian(w, 0.681, 459.0, 26.0, 13.8)
    return x, y, z


def _planck(wavelength_nm: float, teff: float) -> float:
    """Spectral radiance, in whatever units — only the shape matters here."""
    lam = wavelength_nm * 1e-9
    # 2hc^2 and hc/k, SI.
    return (3.7418e-16 / lam**5) / (math.exp(0.014388 / (lam * teff)) - 1.0)


def _linear_to_srgb(value: float) -> float:
    """IEC 61966-2-1 transfer function."""
    v = max(0.0, min(1.0, value))
    return 12.92 * v if v <= 0.0031308 else 1.055 * v ** (1 / 2.4) - 0.055


def blackbody_srgb(teff: float) -> tuple[float, float, float]:
    """A blackbody's hue as sRGB in 0..1, normalised and lifted toward white."""
    x = y = z = 0.0
    # 5 nm over the visible band: finer changes the result by less than a
    # quantisation step, and the CMF fit is smooth at this scale.
    for step in range(0, 81):
        nm = 380.0 + step * 5.0
        radiance = _planck(nm, teff)
        cx, cy, cz = cie_xyz_at(nm)
        x += radiance * cx
        y += radiance * cy
        z += radiance * cz
    total = x + y + z
    if total <= 0:
        return (1.0, 1.0, 1.0)
    x, y, z = x / total, y / total, z / total

    # sRGB / Rec.709 primaries, D65.
    r = 3.2406 * x - 1.5372 * y - 0.4986 * z
    g = -0.9689 * x + 1.8758 * y + 0.0415 * z
    b = 0.0557 * x - 0.2040 * y + 1.0570 * z
    # Out-of-gamut chromaticities (the very red end) come back with a negative
    # channel; desaturating toward the white point is the standard clip and
    # keeps the hue rather than the exposure.
    floor = min(r, g, b)
    if floor < 0:
        r, g, b = r - floor, g - floor, b - floor
    peak = max(r, g, b)
    if peak <= 0:
        return (1.0, 1.0, 1.0)
    r, g, b = r / peak, g / peak, b / peak

    out = [_linear_to_srgb(c) for c in (r, g, b)]
    mean = sum(out) / 3.0
    out = [max(0.0, mean + SATURATION * (c - mean)) for c in out]
    peak = max(out)
    out = [c / peak for c in out]
    return tuple(c + (1.0 - c) * WHITE_LIFT for c in out)  # type: ignore[return-value]


def _level_teff(level: int) -> float:
    """The temperature a ramp level stands for.

    Spaced evenly in 1/T rather than in T: colour changes fast at the cool end
    and barely at all above 10000 K, so even spacing in temperature would spend
    most of the ramp where nothing is visible and band the red stars.
    """
    t = level / (RAMP_LEVELS - 1)
    inv = (1.0 / TEFF_MIN) + t * ((1.0 / TEFF_MAX) - (1.0 / TEFF_MIN))
    return 1.0 / inv


def ramp() -> list[tuple[int, int, int]]:
    """The 24 colours the catalogue is quantised onto, coolest first."""
    out = []
    for level in range(RAMP_LEVELS):
        r, g, b = blackbody_srgb(_level_teff(level))
        out.append((round(255 * r), round(255 * g), round(255 * b)))
    return out


def level_for(teff: float) -> int:
    """Which ramp level a temperature falls on."""
    clamped = max(TEFF_MIN, min(TEFF_MAX, teff))
    t = ((1.0 / clamped) - (1.0 / TEFF_MIN)) / ((1.0 / TEFF_MAX) - (1.0 / TEFF_MIN))
    return max(0, min(RAMP_LEVELS - 1, round(t * (RAMP_LEVELS - 1))))


if __name__ == "__main__":
    for index, rgb in enumerate(ramp()):
        print(f"{index:2d}  {_level_teff(index):7.0f} K  {rgb}")
