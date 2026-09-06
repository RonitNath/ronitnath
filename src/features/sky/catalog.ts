/** The star catalog the landing sky draws.
 *
 * Not a real catalog: a seeded sample of the real *statistics*. Positions are
 * uniform on the sphere and magnitudes follow the observed count law — the
 * number of stars brighter than m goes as 10^0.6m, which is why the sky is a
 * scattering of faint points with a handful of bright ones rather than an even
 * dusting. A real catalog would buy constellations at the cost of shipping a
 * table; the brief takes the generator, and the seed is fixed, so every
 * visitor and every render sees the same sky (docs/design.md: star positions
 * are a designed asset, not something to regenerate per load).
 *
 * Structure-of-arrays because the canvas walks all of it every frame.
 */

/** The seed. Changing it changes the sky, so it does not change. */
export const SKY_SEED = 0x5eed_5c09;

/** Naked-eye limits: Sirius at −1.46, the faintest star a dark sky gives up. */
const MAG_MIN = -1.5;
const MAG_MAX = 6;

export interface StarCatalog {
  /** Right ascension, radians in [0, 2π). */
  ra: Float64Array;
  /** Declination, radians in [−π/2, π/2]. */
  dec: Float64Array;
  /** Apparent magnitude, smaller is brighter. */
  mag: Float64Array;
  /** Colour, 0 = blue-white, 1 = warm — a stand-in for the colour index. */
  tint: Float64Array;
  count: number;
}

/** mulberry32: 32 bits of state, uniform enough for a sky and identical on
 * every engine, which a shared Math.random would not be. */
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

/** Inverse of the cumulative count law, so a uniform draw lands on a
 * magnitude with the right frequency. */
function magnitudeAt(u: number): number {
  const lo = Math.pow(10, 0.6 * MAG_MIN);
  const hi = Math.pow(10, 0.6 * MAG_MAX);
  return Math.log10(u * (hi - lo) + lo) / 0.6;
}

/** Build the sky. Pure in (count, seed): same arguments, same arrays. */
export function generateCatalog(count = 1600, seed: number = SKY_SEED): StarCatalog {
  const random = mulberry32(seed);
  const ra = new Float64Array(count);
  const dec = new Float64Array(count);
  const mag = new Float64Array(count);
  const tint = new Float64Array(count);

  for (let i = 0; i < count; i += 1) {
    ra[i] = random() * Math.PI * 2;
    // asin of a uniform draw, not a uniform declination: the latter would
    // crowd both celestial poles and leave the equator bare.
    dec[i] = Math.asin(random() * 2 - 1);
    mag[i] = magnitudeAt(random());
    // Two draws averaged: most stars sit mid-range, few are strongly coloured.
    tint[i] = (random() + random()) / 2;
  }

  return { ra, dec, mag, tint, count };
}
