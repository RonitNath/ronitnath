import { describe, expect, it } from 'vitest';

import {
  applyView,
  gmstRad,
  J2000_UNIX_MS,
  latLon,
  normalizeLonDeg,
  precessionMatrix,
  SF_LAT_DEG,
  SF_LON_DEG,
  SIDEREAL_DAY_MS,
  subsolarPoint,
  unitVector,
  viewMatrix,
  type Vec3,
} from '../sidereal';
import { SIM_EPOCH_MS } from '../clock';

const dot = (a: readonly number[], b: readonly number[]): number =>
  a[0]! * b[0]! + a[1]! * b[1]! + a[2]! * b[2]!;

const rows = (m: readonly number[]): Vec3[] => [
  [m[0]!, m[3]!, m[6]!],
  [m[1]!, m[4]!, m[7]!],
  [m[2]!, m[5]!, m[8]!],
];

const determinant = (m: Vec3[]): number =>
  m[0]![0] * (m[1]![1] * m[2]![2] - m[1]![2] * m[2]![1]) -
  m[0]![1] * (m[1]![0] * m[2]![2] - m[1]![2] * m[2]![0]) +
  m[0]![2] * (m[1]![0] * m[2]![1] - m[1]![1] * m[2]![0]);

describe('sidereal time', () => {
  it('derives the sidereal day from the rotation rate it is quoted against', () => {
    expect(Math.abs(SIDEREAL_DAY_MS - 86_164_090.5)).toBeLessThan(1);
  });

  it('matches the published GMST at J2000', () => {
    const deg = (gmstRad(J2000_UNIX_MS) * 180) / Math.PI;
    expect(Math.abs(deg - 280.4606)).toBeLessThan(0.01);
  });
});

describe('the view basis', () => {
  it('is orthonormal and deliberately left-handed', () => {
    const basis = rows(viewMatrix(SIM_EPOCH_MS, SF_LAT_DEG, SF_LON_DEG));
    basis.forEach((row, i) => {
      expect(Math.abs(dot(row, row) - 1)).toBeLessThan(1e-9);
      basis
        .slice(i + 1)
        .forEach((other) => expect(Math.abs(dot(row, other))).toBeLessThan(1e-9));
    });
    expect(Math.abs(determinant(basis) + 1)).toBeLessThan(1e-9);
  });

  it('composes Earth rotation exactly once', () => {
    // At J2000 on Greenwich's equator, a star whose RA equals GMST is at
    // zenith. Applying GMST in the observer track as well would fail this.
    const t = 946_727_930_816;
    const ra = gmstRad(t);
    const star: Vec3 = [Math.cos(ra), Math.sin(ra), 0];
    const m = rows(viewMatrix(t, 0, 0));
    expect(Math.abs(dot(m[0]!, star))).toBeLessThan(1e-6);
    expect(Math.abs(dot(m[1]!, star))).toBeLessThan(1e-6);
    expect(Math.abs(dot(m[2]!, star) - 1)).toBeLessThan(1e-6);
  });

  it('puts a star east of the zenith on the screen left', () => {
    const lst = gmstRad(SIM_EPOCH_MS) + (SF_LON_DEG * Math.PI) / 180;
    const star = unitVector(SF_LAT_DEG, ((lst + 0.05) * 180) / Math.PI);
    const [x] = applyView(viewMatrix(SIM_EPOCH_MS, SF_LAT_DEG, SF_LON_DEG), star);
    expect(x).toBeLessThan(0);
  });

  it('lands a known star where an independent calculation says it does', () => {
    // Regulus (HIP 49669), Hipparcos new reduction J2000; the expected values
    // were computed with the scalar equations in Meeus, not with this matrix.
    const check = (unixMs: number, expectedAlt: number, expectedAz: number) => {
      const star = unitVector(11.967_195_190_324_96, 152.093_580_421_387_7);
      const m = rows(viewMatrix(unixMs, SF_LAT_DEG, SF_LON_DEG));
      const altitude = (Math.asin(dot(m[2]!, star)) * 180) / Math.PI;
      const raw = Math.atan2(-dot(m[0]!, star), dot(m[1]!, star));
      const azimuth = ((((raw % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI)) * 180) / Math.PI;
      expect(Math.abs(altitude - expectedAlt)).toBeLessThan(2e-5);
      expect(Math.abs(azimuth - expectedAz)).toBeLessThan(2e-5);
    };
    check(1_785_888_000_000, 46.841_404_782, 243.442_787_112);
    check(4_102_444_800_000, -40.539_846_249, 6.311_664_961);
  });
});

describe('precession', () => {
  it('is the identity at J2000 and a proper rotation away from it', () => {
    expect(precessionMatrix(0)).toEqual([
      [1, 0, 0],
      [0, 1, 0],
      [0, 0, 1],
    ]);
    const p = precessionMatrix(1.25) as unknown as Vec3[];
    p.forEach((row, i) => {
      expect(Math.abs(dot(row, row) - 1)).toBeLessThan(1e-12);
      p.slice(i + 1).forEach((other) => expect(Math.abs(dot(row, other))).toBeLessThan(1e-12));
    });
    expect(Math.abs(determinant(p) - 1)).toBeLessThan(1e-12);
  });
});

describe('coordinates', () => {
  it('round-trips through the unit sphere including the antimeridian', () => {
    for (const [lat, lon] of [
      [37.77, -122.42],
      [-33.87, 151.21],
      [0, 180],
      [64.7, -179],
    ] as const) {
      const [backLat, backLon] = latLon(unitVector(lat, lon));
      expect(Math.abs(backLat - lat)).toBeLessThan(1e-9);
      expect(Math.abs(normalizeLonDeg(backLon - lon))).toBeLessThan(1e-9);
    }
  });
});

describe('the subsolar point', () => {
  it('sits on the tropics at the solstices and on the equator at the equinoxes', () => {
    // 2026 solstices and equinoxes, UTC.
    expect(subsolarPoint(Date.UTC(2026, 5, 21, 8, 24))[0]).toBeCloseTo(23.44, 1);
    expect(subsolarPoint(Date.UTC(2026, 11, 21, 20, 3))[0]).toBeCloseTo(-23.44, 1);
    expect(Math.abs(subsolarPoint(Date.UTC(2026, 2, 20, 14, 46))[0])).toBeLessThan(0.2);
  });

  it('is over the meridian where it is local noon', () => {
    // Noon UTC: the sun is within a degree or so of Greenwich, the offset
    // being the equation of time.
    expect(Math.abs(subsolarPoint(Date.UTC(2026, 3, 15, 12, 0))[1])).toBeLessThan(5);
  });
});
