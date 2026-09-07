import { describe, expect, it } from 'vitest';

import {
  bucketRgb,
  colorBucket,
  FALLOFF,
  haloAlpha,
  sizeTier,
  spriteAlphaField,
  tierSide,
} from '../sprites';

/** The fallback sprite is the star profile's shape, evaluated once per
 * (colour, size, falloff) instead of once per fragment. What has to hold is
 * that it is the same function everywhere in the sprite. */
describe('the point sprite', () => {
  it('carries the halo at the centre, mid-radius and the rim', () => {
    const side = 96;
    const falloff = FALLOFF;
    const field = spriteAlphaField(side, falloff);
    // Row through the middle of the sprite; column offsets are radii.
    const row = side / 2;
    const at = (radius: number): number => {
      const column = Math.round((side / 2) * (1 + radius)) - 1;
      return field[row * side + column]!;
    };
    for (const radius of [0.05, 0.5, 0.9]) {
      // The sprite samples the pixel's centre, so the radius it actually
      // stands at is a pixel-grid one; compare against the formula there.
      const column = Math.round((side / 2) * (1 + radius)) - 1;
      const cx = (2 * (column + 0.5)) / side - 1;
      const cy = (2 * (row + 0.5)) / side - 1;
      expect(at(radius)).toBeCloseTo(haloAlpha(cx * cx + cy * cy, falloff), 2);
    }
  });

  it('is cut off outside the disc, and continuous up to it', () => {
    expect(haloAlpha(1.0001, 2.5)).toBe(0);
    expect(haloAlpha(1, 2.5)).toBeCloseTo(Math.exp(-2.5), 12);
    const side = 32;
    const field = spriteAlphaField(side, 2.5);
    expect(field[0]).toBe(0);
    expect(field[side - 1]).toBe(0);
  });

  it('buckets a size and a colour to within what the eye separates', () => {
    // Two resolutions, resampled by the draw: a dot and a star with a halo.
    expect(sizeTier(1.2)).toBe(0);
    expect(sizeTier(6)).toBe(1);
    expect(tierSide(sizeTier(6))).toBeGreaterThan(tierSide(sizeTier(1.2)));
    expect(colorBucket(1, 1, 1)).not.toBe(colorBucket(0.6, 0.7, 1));
    const [red, green, blue] = bucketRgb(colorBucket(1, 1, 1));
    expect([red, green, blue]).toEqual([255, 255, 255]);
  });
});
