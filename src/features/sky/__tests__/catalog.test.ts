import { describe, expect, it } from 'vitest';

import { generateCatalog, SKY_SEED } from '../catalog';

describe('generateCatalog', () => {
  it('is deterministic: the same seed gives the same sky', () => {
    const a = generateCatalog(500, 1234);
    const b = generateCatalog(500, 1234);
    expect(Array.from(a.ra)).toEqual(Array.from(b.ra));
    expect(Array.from(a.dec)).toEqual(Array.from(b.dec));
    expect(Array.from(a.mag)).toEqual(Array.from(b.mag));
    expect(Array.from(a.tint)).toEqual(Array.from(b.tint));
  });

  it('gives a different sky for a different seed', () => {
    const a = generateCatalog(500, 1234);
    const b = generateCatalog(500, 5678);
    expect(Array.from(a.ra)).not.toEqual(Array.from(b.ra));
  });

  it('holds the shipped sky at the count the canvas draws', () => {
    const sky = generateCatalog();
    expect(sky.count).toBe(2000);
    expect(sky.ra).toHaveLength(2000);
    expect(SKY_SEED).toBe(0x5eed_5c09);
  });

  it('keeps every star inside the coordinate and magnitude ranges', () => {
    const sky = generateCatalog(4000, 99);
    for (let i = 0; i < sky.count; i += 1) {
      expect(sky.ra[i]!).toBeGreaterThanOrEqual(0);
      expect(sky.ra[i]!).toBeLessThan(Math.PI * 2);
      expect(Math.abs(sky.dec[i]!)).toBeLessThanOrEqual(Math.PI / 2);
      expect(sky.mag[i]!).toBeGreaterThanOrEqual(-1.5);
      expect(sky.mag[i]!).toBeLessThanOrEqual(6);
      expect(sky.tint[i]!).toBeGreaterThanOrEqual(0);
      expect(sky.tint[i]!).toBeLessThanOrEqual(1);
    }
  });

  it('follows the observed count law: each magnitude step holds ~4x the last', () => {
    const sky = generateCatalog(200_000, 7);
    const buckets = [0, 0, 0, 0, 0, 0, 0];
    for (let i = 0; i < sky.count; i += 1) {
      const b = Math.floor(sky.mag[i]!) + 1;
      if (b >= 0 && b < buckets.length) buckets[b]! += 1;
    }
    for (let b = 2; b < buckets.length; b += 1) {
      const ratio = buckets[b]! / buckets[b - 1]!;
      // 10^0.6 = 3.98 per magnitude; loose bounds so the test is about the
      // law, not about the sampler's noise.
      expect(ratio).toBeGreaterThan(3);
      expect(ratio).toBeLessThan(5);
    }
  });

  it('spreads stars evenly over the sphere rather than crowding the poles', () => {
    const sky = generateCatalog(100_000, 11);
    // Equal-area bands in sin(dec): four of them should hold a quarter each.
    const bands = [0, 0, 0, 0];
    for (let i = 0; i < sky.count; i += 1) {
      bands[Math.min(3, Math.floor(((Math.sin(sky.dec[i]!) + 1) / 2) * 4))]! += 1;
    }
    for (const n of bands) expect(n / sky.count).toBeCloseTo(0.25, 2);
  });
});
