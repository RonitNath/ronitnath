import { describe, expect, it } from 'vitest';

import { SIM_EPOCH_MS } from '../clock';
import { SF_LAT_DEG, SF_LON_DEG, SIDEREAL_DAY_MS, unitVector } from '../sidereal';
import { greatCircleLerp, observerAt, TRACK_PERIOD_MS, trackBasis } from '../track';

const centralAngleDeg = (
  a: readonly [number, number],
  b: readonly [number, number],
): number => {
  const av = unitVector(a[0], a[1]);
  const bv = unitVector(b[0], b[1]);
  const dot = Math.min(1, Math.max(-1, av[0] * bv[0] + av[1] * bv[1] + av[2] * bv[2]));
  return (Math.acos(dot) * 180) / Math.PI;
};

describe('the travelling observer', () => {
  it('converges on San Francisco at the epoch', () => {
    // A positive epsilon bypasses the exact-phase shortcut.
    expect(
      centralAngleDeg(observerAt(SIM_EPOCH_MS + 0.001), [SF_LAT_DEG, SF_LON_DEG]),
    ).toBeLessThan(1e-6);
    expect(observerAt(SIM_EPOCH_MS)).toEqual([SF_LAT_DEG, SF_LON_DEG]);
  });

  it('keeps every point of the track in one great-circle plane', () => {
    const [start, tangent] = trackBasis();
    const normal = [
      start[1] * tangent[2] - start[2] * tangent[1],
      start[2] * tangent[0] - start[0] * tangent[2],
      start[0] * tangent[1] - start[1] * tangent[0],
    ];
    for (let i = 1; i < 64; i += 1) {
      const point = observerAt(SIM_EPOCH_MS + (TRACK_PERIOD_MS * i) / 64);
      const v = unitVector(point[0], point[1]);
      const error = normal[0]! * v[0] + normal[1]! * v[1] + normal[2]! * v[2];
      expect(Math.abs(error)).toBeLessThan(1e-12);
    }
  });

  it('moves at a constant angular speed', () => {
    const step = TRACK_PERIOD_MS / 16;
    for (let i = 0; i < 16; i += 1) {
      const a = observerAt(SIM_EPOCH_MS + 123 + step * i);
      const b = observerAt(SIM_EPOCH_MS + 123 + step * (i + 1));
      expect(Math.abs(centralAngleDeg(a, b) - 22.5)).toBeLessThan(1e-6);
    }
  });

  it('reaches the documented latitude spread', () => {
    let [min, max] = [90, -90];
    for (let i = 0; i < 720; i += 1) {
      const [lat] = observerAt(SIM_EPOCH_MS + (TRACK_PERIOD_MS * i) / 720);
      min = Math.min(min, lat);
      max = Math.max(max, lat);
    }
    expect(max).toBeGreaterThan(62.9);
    expect(min).toBeLessThan(-62.9);
  });

  it('laps a whole number of times per resync, so the observer never teleports', () => {
    const laps = SIDEREAL_DAY_MS / TRACK_PERIOD_MS;
    expect(Math.abs(laps - Math.round(laps))).toBeLessThan(1e-9);
    const seam = SIM_EPOCH_MS + SIDEREAL_DAY_MS;
    expect(centralAngleDeg(observerAt(seam - 0.001), observerAt(seam + 0.001))).toBeLessThan(
      1e-6,
    );
  });
});

describe('a manual move', () => {
  it('takes the shorter way round the antimeridian', () => {
    const midpoint = greatCircleLerp([0, 0], [0, 90], 0.5);
    expect(Math.abs(midpoint[0])).toBeLessThan(1e-9);
    expect(Math.abs(midpoint[1] - 45)).toBeLessThan(1e-9);

    const across = greatCircleLerp([0, 170], [0, -170], 0.5);
    expect(Math.abs(Math.abs(across[1]) - 180)).toBeLessThan(1e-9);
  });

  it('lands exactly on its endpoints', () => {
    const [a, b] = [
      [10, 20],
      [-40, 130],
    ] as const;
    expect(greatCircleLerp(a, b, 0)[0]).toBeCloseTo(a[0], 9);
    expect(greatCircleLerp(a, b, 1)[1]).toBeCloseTo(b[1], 9);
  });

  it('is a no-op onto the same point rather than a division by zero', () => {
    const here: [number, number] = [37.77, -122.42];
    const [lat, lon] = greatCircleLerp(here, here, 0.5);
    expect(Number.isFinite(lat) && Number.isFinite(lon)).toBe(true);
    expect(Math.abs(lat - here[0])).toBeLessThan(1e-9);
  });
});
