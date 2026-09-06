import { describe, expect, it } from 'vitest';

import { altAz, gmstRad, J2000_UNIX_MS, lstRad, SIDEREAL_DAY_MS } from '../sidereal';

const DEG = Math.PI / 180;

describe('gmstRad', () => {
  it('matches the published value at J2000.0', () => {
    // 2000-01-01T12:00Z: GMST = 18h 41m 50.5s = 280.4606°.
    expect(gmstRad(J2000_UNIX_MS) / DEG).toBeCloseTo(280.4606, 3);
  });

  it('returns to the same angle one sidereal day later', () => {
    const a = gmstRad(J2000_UNIX_MS);
    const b = gmstRad(J2000_UNIX_MS + SIDEREAL_DAY_MS);
    expect(Math.abs(a - b)).toBeLessThan(1e-9);
  });

  it('derives the textbook sidereal day, 23h56m04.09s', () => {
    expect(SIDEREAL_DAY_MS).toBeCloseTo(86_164_090.5, -1);
  });

  it('stays in [0, 2π) on both sides of the epoch', () => {
    for (const t of [0, J2000_UNIX_MS, 1.9e12, -1e12]) {
      const g = gmstRad(t);
      expect(g).toBeGreaterThanOrEqual(0);
      expect(g).toBeLessThan(Math.PI * 2);
    }
  });
});

describe('lstRad', () => {
  it('is GMST at Greenwich', () => {
    expect(lstRad(J2000_UNIX_MS, 0)).toBeCloseTo(gmstRad(J2000_UNIX_MS), 12);
  });

  it('runs one hour of angle behind for every 15° west', () => {
    const t = 1_756_000_000_000;
    const west = lstRad(t, -15);
    expect(west).toBeCloseTo((gmstRad(t) - 15 * DEG + Math.PI * 2) % (Math.PI * 2), 12);
  });
});

describe('altAz', () => {
  it('puts the celestial pole due north at an altitude equal to the latitude', () => {
    for (const latDeg of [0, 12.5, 37.7749, 51.5, 89]) {
      const { alt, az } = altAz(0, Math.PI / 2, 1.234, latDeg * DEG);
      expect(alt / DEG).toBeCloseTo(latDeg, 9);
      // Due north is 0 or 2π depending on which side of it float error lands.
      expect(Math.min(az, Math.PI * 2 - az)).toBeCloseTo(0, 9);
    }
  });

  it('puts the south celestial pole due south below a northern horizon', () => {
    const { alt, az } = altAz(0, -Math.PI / 2, 2.5, 37.7749 * DEG);
    expect(alt / DEG).toBeCloseTo(-37.7749, 9);
    expect(az / DEG).toBeCloseTo(180, 9);
  });

  it('puts a star on the meridian at the observer declination in the zenith', () => {
    const lat = 37.7749 * DEG;
    const lst = 3.1;
    const { alt } = altAz(lst, lat, lst, lat);
    expect(alt).toBeCloseTo(Math.PI / 2, 9);
  });

  it('places a rising star in the east and a setting one in the west', () => {
    const lat = 37.7749 * DEG;
    const lst = 1;
    const rising = altAz(lst + 0.4, 0, lst, lat); // hour angle < 0
    const setting = altAz(lst - 0.4, 0, lst, lat); // hour angle > 0
    expect(rising.az / DEG).toBeGreaterThan(0);
    expect(rising.az / DEG).toBeLessThan(180);
    expect(setting.az / DEG).toBeGreaterThan(180);
  });

  it('carries a star all the way round in one sidereal day and no less', () => {
    const lat = 37.7749 * DEG;
    const ra = 2.2;
    const dec = 0.3;
    const t = 1_756_000_000_000;
    const before = altAz(ra, dec, lstRad(t, -122.4194), lat);
    const after = altAz(ra, dec, lstRad(t + SIDEREAL_DAY_MS, -122.4194), lat);
    expect(after.alt).toBeCloseTo(before.alt, 9);
    expect(after.az).toBeCloseTo(before.az, 9);

    const halfway = altAz(ra, dec, lstRad(t + SIDEREAL_DAY_MS / 2, -122.4194), lat);
    expect(Math.abs(halfway.alt - before.alt)).toBeGreaterThan(0.1);
  });
});
