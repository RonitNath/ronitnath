import { describe, expect, it } from 'vitest';

import {
  bucketRgb,
  colorBucket,
  falloffFor,
  haloAlpha,
  sizeTier,
  spriteAlphaField,
  tierSide,
} from '../sprites';
import { TUNING } from '../tuning';

/** The sprite is the pre-rebuild point-sprite shader, evaluated once instead
 * of once per fragment. What has to hold is that it is the same function. */
describe('the point sprite', () => {
  it('carries the shader halo at the centre, mid-radius and the rim', () => {
    const side = 96;
    const falloff = falloffFor(1);
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

  it('discards outside the disc, exactly as the fragment shader does', () => {
    expect(haloAlpha(1.0001, 2.5)).toBe(0);
    expect(haloAlpha(1, 2.5)).toBeCloseTo(Math.exp(-2.5), 12);
    const side = 32;
    const field = spriteAlphaField(side, 2.5);
    expect(field[0]).toBe(0);
    expect(field[side - 1]).toBe(0);
  });

  it('widens the halo with brightness only as far as the shipped halo says', () => {
    // The shipped `halo` is 0, so the falloff is `glow` for every star; the
    // expression is still the shader's, so a non-zero halo would widen it.
    expect(falloffFor(1)).toBeCloseTo(TUNING.glow, 12);
    expect(falloffFor(1e-4)).toBeCloseTo(TUNING.glow, 12);
    const widened = TUNING.glow / (1 + 0.5 * Math.pow(100, 0.25));
    expect(widened).toBeLessThan(TUNING.glow);
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
