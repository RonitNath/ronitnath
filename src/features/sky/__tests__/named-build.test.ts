import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { parseNamed, parseStars } from '../catalog';
import { CONSTELLATIONS } from '../constellations';

const NAMED = parseNamed(readFileSync('public/stars/named.json', 'utf8'));
const STARS = parseStars(new Uint8Array(readFileSync('public/stars/bright.bin')));
const HAND = JSON.parse(
  readFileSync('tools/starcat/data/named_hand.json', 'utf8'),
) as { stars: { name: string; classification: string; distanceLy: number }[] };

describe('the grown name list', () => {
  it('is the whole IAU set that resolves, not the fifty S1 wrote', () => {
    expect(NAMED.stars.length).toBeGreaterThan(300);
    // 339 of the 451 approved names are brighter than the catalogue's G <= 6.5
    // cut, and six of those are stars Gaia saturates on; the rest resolve.
    expect(NAMED.stars.length).toBeLessThanOrEqual(339);
  });

  it('carries a two-word name whole', () => {
    // S3 read the IAU file with a whitespace regex over fixed columns, so
    // "Rigil Kentaurus" arrived as "Rigil" with "Kentaurus" in the diacritics
    // column. Twenty-four of the names are two words.
    const names = new Set(NAMED.stars.map((star) => star.name));
    expect(names.has('Rigil Kentaurus')).toBe(true);
    expect(names.has('Rigil')).toBe(false);
    expect([...names].filter((name) => name.includes(' ')).length).toBeGreaterThan(10);
  });

  it('names one star per record and every record it names', () => {
    const indices = NAMED.stars.map((star) => star.brightIndex);
    expect(new Set(indices).size).toBe(indices.length);
    for (const index of indices) expect(index).toBeLessThan(STARS.count);
  });

  it('says a constellation the panel can also say', () => {
    const names = new Set(Object.values(CONSTELLATIONS));
    for (const star of NAMED.stars) expect(names.has(star.constellation)).toBe(true);
  });

  it('keeps the hand-checked text where the star kept its hand-written name', () => {
    const built = new Map(NAMED.stars.map((star) => [star.name, star]));
    let kept = 0;
    for (const hand of HAND.stars) {
      const star = built.get(hand.name);
      if (!star) continue; // superseded by the star's IAU name; two of them.
      expect(star.classification).toBe(hand.classification);
      expect(star.distanceLy).toBe(hand.distanceLy);
      kept += 1;
    }
    expect(kept).toBeGreaterThanOrEqual(45);
  });

  it('rounds a generated distance the way the fifty were rounded', () => {
    // A tenth under 100 ly, ten light years under 1,000, a hundred beyond.
    // The hand-written entries keep whatever a person read off SIMBAD, so
    // they are not held to the rule the builder follows.
    const hand = new Set(HAND.stars.map((star) => star.name));
    for (const star of NAMED.stars) {
      expect(star.distanceLy).toBeGreaterThan(0);
      if (hand.has(star.name)) continue;
      const scale = star.distanceLy < 100 ? 10 : star.distanceLy < 1_000 ? 0.1 : 0.01;
      const steps = star.distanceLy * scale;
      expect(Math.abs(steps - Math.round(steps))).toBeLessThan(1e-6);
    }
  });
});
