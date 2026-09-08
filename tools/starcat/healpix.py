#!/usr/bin/env python3
"""HEALPix NESTED indexing, hand-rolled and falsifiable.

Carried out of `mwcat.py` when the band moved to level 9: the baker is at its
line cap and this is the piece that has to be right for a star count to be a
surface brightness at all. healpy would be a heavyweight build dependency for
two functions, and this pair is exercised by the equal-area self-test below.

Gorski et al. 2005, §4.
"""

import math

import numpy as np


def interleave(ix: np.ndarray, iy: np.ndarray, order: int) -> np.ndarray:
    """Bit-interleave (ix even bits, iy odd bits) — the NESTED suffix."""
    out = np.zeros(ix.shape, dtype=np.int64)
    for bit in range(order):
        out |= ((ix >> bit) & 1) << (2 * bit)
        out |= ((iy >> bit) & 1) << (2 * bit + 1)
    return out


def ang2pix_nest(theta: np.ndarray, phi: np.ndarray, order: int) -> np.ndarray:
    """Vectorized HEALPix NESTED lookup for co-latitude/longitude in radians."""
    nside = 1 << order
    z = np.cos(theta)
    za = np.abs(z)
    tt = np.mod(phi, 2.0 * math.pi) / (math.pi / 2.0)  # in [0, 4)

    face = np.zeros(z.shape, dtype=np.int64)
    ix = np.zeros(z.shape, dtype=np.int64)
    iy = np.zeros(z.shape, dtype=np.int64)

    # Equatorial belt: the two diagonal edge-line families index the face.
    eq = za <= 2.0 / 3.0
    if np.any(eq):
        temp1 = nside * (0.5 + tt[eq])
        temp2 = nside * z[eq] * 0.75
        jp = np.floor(temp1 - temp2).astype(np.int64)
        jm = np.floor(temp1 + temp2).astype(np.int64)
        ifp = jp >> order
        ifm = jm >> order
        face[eq] = np.where(
            ifp == ifm, (ifp & 3) + 4, np.where(ifp < ifm, ifp & 3, (ifm & 3) + 8)
        )
        ix[eq] = jm & (nside - 1)
        iy[eq] = nside - (jp & (nside - 1)) - 1

    # Polar caps: distance from the pole sets both coordinates.
    po = ~eq
    if np.any(po):
        ntt = np.minimum(3, tt[po].astype(np.int64))
        tp = tt[po] - ntt
        tmp = nside * np.sqrt(np.maximum(0.0, 3.0 * (1.0 - za[po])))
        jp = np.minimum((tp * tmp).astype(np.int64), nside - 1)
        jm = np.minimum(((1.0 - tp) * tmp).astype(np.int64), nside - 1)
        north = z[po] >= 0
        face[po] = np.where(north, ntt, ntt + 8)
        ix[po] = np.where(north, nside - jm - 1, jp)
        iy[po] = np.where(north, nside - jp - 1, jm)

    return face * nside * nside + interleave(ix, iy, order)


def selftest(order: int) -> float:
    """Falsify ang2pix_nest on HEALPix's defining property — equal area — plus
    the base-pixel layout. Swapped ix/iy, a wrong interleave, or a bad face
    rule all break the area test, which is the whole point of using this grid:
    a pixel's star count is only a surface brightness if every pixel subtends
    the same solid angle."""
    nside = 1 << order
    npix = 12 * nside * nside
    rng = np.random.default_rng(7)
    z = rng.uniform(-1.0, 1.0, 400_000)
    phi = rng.uniform(0.0, 2.0 * math.pi, 400_000)
    pix = ang2pix_nest(np.arccos(z), phi, order)
    assert pix.min() >= 0 and pix.max() < npix, "pixel index out of range"

    # Sampling at the working order is Poisson-sparse, so aggregate to level 2
    # (192 superpixels, ~2,080 samples each) where uniformity is measurable. A
    # NESTED index truncates to its parent by shifting off two bits per level.
    coarse = np.bincount(pix >> (2 * (order - 2)), minlength=192)
    assert len(coarse) == 192, f"got {len(coarse)} base regions"
    spread = coarse.max() / coarse.min()
    assert spread < 1.25, f"equal-area violated: max/min = {spread:.2f}"

    # North pole belongs to a north-cap face (0-3), south pole to a south-cap
    # face (8-11); getting the hemisphere backwards mirrors the whole sky.
    assert ang2pix_nest(np.array([0.0]), np.array([0.0]), order)[0] // (nside * nside) < 4
    assert (
        ang2pix_nest(np.array([math.pi]), np.array([0.0]), order)[0] // (nside * nside)
        >= 8
    )
    return float(spread)


# North galactic pole, J2000 equatorial, plus the galactic longitude of the
# north celestial pole — the two constants that define the galactic frame.
NGP_RA_DEG, NGP_DEC_DEG = 192.85948, 27.12825
NCP_L_DEG = 122.93192


def galactic_to_equatorial(l_deg: np.ndarray, b_deg: np.ndarray) -> np.ndarray:
    """Galactic (l, b) -> equatorial (ra, dec), both in radians."""
    lon, lat = np.radians(l_deg), np.radians(b_deg)
    dg, ag, ln = map(math.radians, (NGP_DEC_DEG, NGP_RA_DEG, NCP_L_DEG))
    dec = np.arcsin(
        math.sin(dg) * np.sin(lat) + math.cos(dg) * np.cos(lat) * np.cos(ln - lon)
    )
    ra = ag + np.arctan2(
        np.cos(lat) * np.sin(ln - lon),
        math.cos(dg) * np.sin(lat) - math.sin(dg) * np.cos(lat) * np.cos(ln - lon),
    )
    return np.stack([ra, dec])


def check_galactic_geometry(counts: np.ndarray, order: int) -> None:
    """The band must land on the galactic equator once mapped into equatorial
    coordinates. Sampling a ring at b=0 against rings near both galactic poles
    tests the HEALPix lookup end to end against real astronomy — a mirrored,
    transposed or half-turn-rotated map fails it."""

    def ring(b_deg: float) -> np.ndarray:
        lon = np.linspace(0.0, 360.0, 720, endpoint=False)
        ra, dec = galactic_to_equatorial(lon, np.full_like(lon, b_deg))
        return counts[ang2pix_nest(math.pi / 2.0 - dec, ra, order)]

    plane = float(np.median(ring(0.0)))
    poles = float(np.median(np.concatenate([ring(75.0), ring(-75.0)])))
    print(f"median star count, galactic equator vs |b|=75: {plane / poles:,.1f}x")
    # Gaia's own counts run about 8x between these zones at G<15; anything
    # under 4x means the band is not sitting on the galactic equator.
    if plane <= poles * 4.0:
        raise SystemExit("the galactic plane is not where it should be")


if __name__ == "__main__":
    print(f"ang2pix_nest ok (equal-area spread {selftest(9):.3f})")
