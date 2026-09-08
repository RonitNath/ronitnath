/** The stars the catalogue was missing, and the ones it must not have twice.
 *
 * Gaia DR3 is unusable above G ~ 1.7 and unreliable well past it, and the
 * first build's Hipparcos supplement was cut at Hp < 2.5. Everything in
 * between belonged to neither half: five second-magnitude stars — Sheratan,
 * Menkar, Mahasim, Gienah and Enif — were drawn nowhere at all, and eleven
 * constellation segments were dropped for want of them.
 *
 * `build_bright.py` now fills that band from the whole naked-eye Hipparcos
 * catalogue. These are the assertions that say the fill happened, that it
 * happened once per star, and that the figures closed behind it.
 */
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { SKY_ASSETS } from '../asset-names';
import { KIND_HIP, parseNamed, parseStars } from '../catalog';
import { parseLines } from '../lines';
import { shipped, shippedText } from './fixtures/shipped';

const STARS = parseStars(new Uint8Array(shipped(SKY_ASSETS.bright)));
const NAMED = parseNamed(shippedText(SKY_ASSETS.named));
const LINES = parseLines(new Uint8Array(shipped(SKY_ASSETS.lines)));
const LINES_MANIFEST = JSON.parse(readFileSync('public/sky/lines.json', 'utf8')) as {
  pairs: number;
  droppedPairs: number;
  unresolvedHip: number[];
  catalogue: string;
};
const NAMED_CATALOGUE = (JSON.parse(shippedText(SKY_ASSETS.named)) as { catalogue: string })
  .catalogue;
const FILLED = JSON.parse(readFileSync('tools/starcat/data/hip_filled.json', 'utf8')) as {
  stars: { hip: number; hpMag: number; vec: [number, number, number] }[];
};

/** The five the S4 gate found missing, with the Hipparcos numbers, positions
 * and colours their catalogue rows carry. B−V is what decides the drawn
 * colour for a Hipparcos record: Menkar and Enif are red giants, the other
 * three white or blue-white. */
const MISSING = [
  { name: 'Sheratan', hip: 8903, ra: 28.659789, dec: 20.8083, warm: false },
  { name: 'Menkar', hip: 14135, ra: 45.569913, dec: 4.089926, warm: true },
  { name: 'Mahasim', hip: 28380, ra: 89.930159, dec: 37.212764, warm: false },
  { name: 'Gienah', hip: 59803, ra: 183.951949, dec: -17.541984, warm: false },
  { name: 'Enif', hip: 107315, ra: 326.046417, dec: 9.875008, warm: true },
];

function unit(raDeg: number, decDeg: number): [number, number, number] {
  const ra = (raDeg * Math.PI) / 180;
  const dec = (decDeg * Math.PI) / 180;
  return [Math.cos(dec) * Math.cos(ra), Math.cos(dec) * Math.sin(ra), Math.sin(dec)];
}

/** Where each record points, re-normalised once.
 *
 * The catalogue stores f32, whose norm is only 1 to about six parts in a
 * hundred million. Comparing raw dot products at these separations ranks a
 * star four arcseconds away above the star itself, because the norm error is
 * larger than the angle — which is the trap `build_bright.py` documents and
 * the reason every distance here comes from a chord between unit vectors.
 */
const UNIT = (() => {
  const out = new Float64Array(STARS.count * 3);
  for (let star = 0; star < STARS.count; star += 1) {
    const at = star * 3;
    const [x, y, z] = [STARS.position[at]!, STARS.position[at + 1]!, STARS.position[at + 2]!];
    const norm = Math.hypot(x, y, z) || 1;
    out[at] = x / norm;
    out[at + 1] = y / norm;
    out[at + 2] = z / norm;
  }
  return out;
})();

function arcsecTo(target: readonly number[], index: number): number {
  const at = index * 3;
  const chord = Math.hypot(
    target[0]! - UNIT[at]!,
    target[1]! - UNIT[at + 1]!,
    target[2]! - UNIT[at + 2]!,
  );
  return (2 * Math.asin(Math.min(1, chord / 2)) * 180 * 3_600) / Math.PI;
}

/** Every record within `arcsec` of a direction. */
function within(target: readonly number[], arcsec: number): number[] {
  const found: number[] = [];
  for (let star = 0; star < STARS.count; star += 1) {
    if (arcsecTo(target, star) <= arcsec) found.push(star);
  }
  return found;
}

/** Record index by Hipparcos number, for the records the fill contributed. */
const BY_HIP = new Map<number, number>();
for (let star = 0; star < STARS.count; star += 1) {
  if (STARS.kind[star] === KIND_HIP) BY_HIP.set(Number(STARS.id[star]), star);
}

describe('the filled catalogue', () => {
  it('draws the five naked-eye stars that were in neither half', () => {
    for (const star of MISSING) {
      const index = BY_HIP.get(star.hip);
      expect(`${star.name} in the catalogue`).toBe(
        index === undefined ? `${star.name} missing` : `${star.name} in the catalogue`,
      );
      // The record is carried to Gaia's epoch, so it sits a couple of
      // arcseconds off the 1991.25 position its catalogue row gives.
      expect(arcsecTo(unit(star.ra, star.dec), index!)).toBeLessThan(30);
      // Second magnitude, not the magnitude-7 saturated Gaia row Mahasim's
      // position used to carry in the deep layer.
      expect(STARS.magnitude[index!]).toBeLessThan(2.8);
      // Warm stars sit at the red end of the 24-level ramp, which is where
      // red is more than blue by a wide margin.
      const at = index! * 3;
      const warm = STARS.color[at]! - STARS.color[at + 2]! > 0.2;
      expect(warm).toBe(star.warm);
    }
  });

  it('adds each filled star exactly once, and never on top of a Gaia row', () => {
    // Two records of one star draw it twice, at two magnitudes: HIP 92862 at
    // 3.93 with a saturated Gaia ghost at 2.36 two arcseconds away. The
    // build's cross-match radius is 3 arcsec, so that is what must be empty
    // apart from the star itself.
    const filled = new Set(FILLED.stars.map((star) => star.hip));
    expect(filled.size).toBe(FILLED.stars.length);
    for (const star of FILLED.stars) {
      const index = BY_HIP.get(star.hip);
      expect(index).toBeDefined();
      expect(arcsecTo(star.vec, index!)).toBeLessThan(1);
      expect(within(star.vec, 3)).toEqual([index]);
    }
  });

  it('names and figures were resolved against the catalogue that shipped', () => {
    /* Both address stars by index, and every record the catalogue gains moves
     * the ones after it — so a rebuilt `bright.bin` beside a stale name list
     * labels the wrong stars, silently and plausibly. Each builder writes down
     * the content hash of the catalogue it read; the catalogue's own filename
     * carries the same hash, so the three files can say whether they were
     * built together. */
    const catalogue = SKY_ASSETS.bright.replace(/^.*-|\.bin$/g, '');
    expect(catalogue).toHaveLength(12);
    expect(NAMED_CATALOGUE).toBe(catalogue);
    expect(LINES_MANIFEST.catalogue).toBe(catalogue);
  });

  it('resolves every named star and every line endpoint into a record', () => {
    for (const star of NAMED.stars) {
      expect(star.brightIndex).toBeGreaterThanOrEqual(0);
      expect(star.brightIndex).toBeLessThan(STARS.count);
    }
    for (const index of LINES) expect(index).toBeLessThan(STARS.count);
  });

  it('draws 674 of Stellarium’s 676 segments, and says which two it cannot', () => {
    // The two it cannot both run through HIP 33165, a V 6.65 star in Canis
    // Major that is fainter than the catalogue's own magnitude 6.5 limit. The
    // other nine S4 dropped are now drawn.
    expect(LINES_MANIFEST.pairs).toBe(674);
    expect(LINES_MANIFEST.droppedPairs).toBe(2);
    expect(LINES_MANIFEST.unresolvedHip).toEqual([33165]);
  });
});
