import { describe, expect, it } from 'vitest';

import { SIM_EPOCH_MS, SPEED, simTimeMs, syncedSimTimeMs } from '../clock';
import { SF_LAT_DEG, SF_LON_DEG, SIDEREAL_DAY_MS, viewMatrix } from '../sidereal';

describe('the simulation clock', () => {
  it('runs exactly sixty times wall clock', () => {
    const delta = 12_345;
    expect(simTimeMs(SIM_EPOCH_MS + delta) - simTimeMs(SIM_EPOCH_MS)).toBeCloseTo(
      SPEED * delta,
      6,
    );
  });

  it('is a pure function of the wall clock', () => {
    for (const now of [SIM_EPOCH_MS, 1_800_000_000_000, 2_000_000_000_000]) {
      expect(simTimeMs(now)).toBe(simTimeMs(now));
    }
  });

  it('never lets the simulated date run away from the real one', () => {
    // The bug this replaces: an unbounded 60x clock reached 2061 within months
    // of launch. Sample a real year of wall time.
    const yearMs = 365.25 * 86_400_000;
    for (let i = 0; i < 2_000; i += 1) {
      const now = SIM_EPOCH_MS + (yearMs * i) / 2_000;
      const lead = simTimeMs(now) - now;
      expect(lead).toBeGreaterThanOrEqual(0);
      expect(lead).toBeLessThan(SIDEREAL_DAY_MS);
    }
  });

  it('leaves the sky identical across the resync seam', () => {
    // The invariant the resync rests on. Only precession survives the
    // subtraction, and over one day that is 0.06 arcsec.
    for (let sample = 0; sample < 8; sample += 1) {
      const t = SIM_EPOCH_MS + 987_654_321 * sample;
      const now = viewMatrix(t, SF_LAT_DEG, SF_LON_DEG);
      const earlier = viewMatrix(t - SIDEREAL_DAY_MS, SF_LAT_DEG, SF_LON_DEG);
      now.forEach((value, i) => expect(Math.abs(value - earlier[i]!)).toBeLessThan(1e-5));
    }
  });

  it('is continuous either side of the seam the modulus creates', () => {
    // Cross the instant at which the accumulated lead wraps: the *matrices*
    // must match, even though the simulated dates differ by a sidereal day.
    const wrapAt = SIM_EPOCH_MS + SIDEREAL_DAY_MS / (SPEED - 1);
    const before = simTimeMs(wrapAt - 1);
    const after = simTimeMs(wrapAt + 1);
    expect(Math.abs(after - before)).toBeGreaterThan(SIDEREAL_DAY_MS / 2);
    const a = viewMatrix(before, SF_LAT_DEG, SF_LON_DEG);
    const b = viewMatrix(after, SF_LAT_DEG, SF_LON_DEG);
    a.forEach((value, i) => expect(Math.abs(value - b[i]!)).toBeLessThan(1e-4));
  });
});

describe('the server sync', () => {
  it('reads the server instant when the client clock is right', () => {
    const now = 1_800_000_000_000;
    expect(syncedSimTimeMs(now, now, now)).toBe(simTimeMs(now));
  });

  it('carries a wrong client clock back onto the server one', () => {
    const server = 1_800_000_000_000;
    const clientMount = server + 7 * 86_400_000; // A week fast.
    const elapsed = 5_000;
    expect(syncedSimTimeMs(server, clientMount, clientMount + elapsed)).toBe(
      simTimeMs(server + elapsed),
    );
  });
});
