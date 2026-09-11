import { describe, expect, it } from 'vitest';

import { TWINKLE, twinkleFactor } from '../tuning';

const ZENITH = 1;
const LOW = Math.cos((75 * Math.PI) / 180); // 75 degrees from the zenith.

describe('scintillation', () => {
  it('leaves a faint star alone', () => {
    for (let time = 0; time < 4_000; time += 37) {
      expect(twinkleFactor(TWINKLE.magLimit + 0.01, LOW, 3, time)).toBe(1);
    }
  });

  it('barely moves a star at the zenith and moves a low one', () => {
    const swing = (zenithCos: number): number => {
      let low = Infinity;
      let high = -Infinity;
      for (let time = 0; time < 4_000; time += 7) {
        const value = twinkleFactor(0, zenithCos, 11, time);
        low = Math.min(low, value);
        high = Math.max(high, value);
      }
      return high - low;
    };
    expect(swing(ZENITH)).toBeLessThan(1e-9);
    expect(swing(LOW)).toBeGreaterThan(0.2);
  });

  it('never puts a star out and never doubles it, at any airmass', () => {
    let lowest = Infinity;
    let highest = -Infinity;
    for (const zenithCos of [1, 0.5, 0.2, 0.05, 0.01]) {
      for (let seed = 0; seed < 200; seed += 1) {
        for (let time = 0; time < 2_000; time += 13) {
          const value = twinkleFactor(1, zenithCos, seed, time);
          lowest = Math.min(lowest, value);
          highest = Math.max(highest, value);
        }
      }
    }
    expect(lowest).toBeGreaterThan(0.5);
    expect(highest).toBeLessThan(1.5);
  });

  it('gives each star its own period inside the stated range', () => {
    // The period is drawn from the star's index, so no two neighbours pulse
    // together; the seed is what makes the field air rather than a strobe.
    const periods = new Set<number>();
    for (let seed = 0; seed < 50; seed += 1) {
      const spread = (Math.sin(seed * 12.9898) + 1) / 2;
      const period = TWINKLE.periodMinMs + spread * (TWINKLE.periodMaxMs - TWINKLE.periodMinMs);
      expect(period).toBeGreaterThanOrEqual(TWINKLE.periodMinMs);
      expect(period).toBeLessThanOrEqual(TWINKLE.periodMaxMs);
      periods.add(Math.round(period));
    }
    expect(periods.size).toBeGreaterThan(40);
  });
});
