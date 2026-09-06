import { describe, expect, it } from 'vitest';

import { bandUv } from '../star-field';
import { applyView, FOCAL, gmstRad, type Vec3, viewMatrix } from '../sidereal';

const DEG = Math.PI / 180;

/** A J2000 direction as a unit vector, from right ascension and declination. */
function direction(raHours: number, decDeg: number): Vec3 {
  const ra = raHours * 15 * DEG;
  const dec = decDeg * DEG;
  return [Math.cos(dec) * Math.cos(ra), Math.cos(dec) * Math.sin(ra), Math.sin(dec)];
}

/** The Milky Way map is baked in equatorial coordinates, so the band pass is
 * only right if the pixel a direction is drawn at samples the map where that
 * direction is. This is the round trip: project a known star through the star
 * pass, then ask the band pass what it reads there. */
describe('the band pass', () => {
  it('samples the map where the direction it is drawing actually is', () => {
    // Sgr A*, the galactic centre and the brightest thing on the map.
    const [ra, dec] = [17 + 45 / 60 + 40 / 3600, -29.0078];
    const target = direction(ra, dec);

    // Stand under it: latitude equal to its declination, longitude such that
    // local sidereal time equals its right ascension, and it is at the zenith.
    const unixMs = Date.UTC(2026, 5, 21, 6, 0, 0);
    const lonDeg = ((ra * 15 - gmstRad(unixMs) / DEG) % 360) - 360;
    const matrix = viewMatrix(unixMs, dec, lonDeg);
    const [vx, vy, vz] = applyView(matrix, target);
    expect(vz).toBeGreaterThan(0.9);

    const aspect = 1440 / 900;
    const ndcX = ((vx / vz) * FOCAL) / aspect;
    const ndcY = (vy / vz) * FOCAL;
    const [u, v, rayZ] = bandUv(matrix, ndcX, ndcY, aspect);

    // Equirectangular: u is right ascension around, v is declination down.
    expect(u).toBeCloseTo(((ra * 15 * DEG) / (2 * Math.PI) + 0.5) % 1, 4);
    expect(v).toBeCloseTo(0.5 - (dec * DEG) / Math.PI, 4);
    expect(rayZ).toBeGreaterThan(0.9);

    // And the map pixel it lands on, for the shipped 1024x512 bake.
    expect(Math.floor(u * 1024)).toBe(Math.floor((((ra * 15) / 360 + 0.5) % 1) * 1024));
    expect(Math.floor(v * 512)).toBe(Math.floor((0.5 - dec / 180) * 512));
  });

  it('reads the zenith at the frame centre and the horizon at its edge', () => {
    const matrix = viewMatrix(Date.UTC(2026, 0, 1), 37.7749, -122.4194);
    const [, , centre] = bandUv(matrix, 0, 0, 1.6);
    const [, , corner] = bandUv(matrix, 1, 1, 1.6);
    expect(centre).toBeCloseTo(1, 12);
    expect(corner).toBeLessThan(0.45);
    expect(corner).toBeGreaterThan(0.3);
  });
});
