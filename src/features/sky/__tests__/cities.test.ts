import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { CityCatalog, distanceBetweenKm, EARTH_MEAN_RADIUS_KM } from '../cities';

const BYTES = new Uint8Array(readFileSync('public/cities/cities.bin'));
const catalog = (): CityCatalog => CityCatalog.parse(BYTES)!;

describe('the shipped city catalog', () => {
  it('parses and has its filtered size', () => {
    expect(catalog().length).toBe(4_063);
  });

  it('resolves hand-checked land points to the expected city', () => {
    const cities = catalog();
    for (const [lat, lon, name, country, withinKm] of [
      [40.73, -73.99, 'New York City', 'US', 5],
      [35.6895, 139.6917, 'Tokyo', 'JP', 1],
      [-33.87, 151.21, 'Sydney', 'AU', 10],
    ] as const) {
      const city = cities.nearest(lat, lon)!;
      expect([city.name, city.country]).toEqual([name, country]);
      expect(city.distanceKm).toBeLessThan(withinKm);
    }
  });

  it('finds the city just east of the antimeridian from just west of it', () => {
    const city = catalog().nearest(64.7, -179)!;
    expect([city.name, city.country]).toEqual(['Anadyr', 'RU']);
    expect(city.distanceKm).toBeLessThan(200);
  });

  it('reports an honest distance over the open ocean rather than nothing', () => {
    const city = catalog().nearest(0, -140)!;
    expect([city.name, city.country]).toEqual(['Honolulu', 'US']);
    expect(city.distanceKm).toBeGreaterThan(2_000);
  });

  it('refuses a truncated or tampered asset whole', () => {
    expect(CityCatalog.parse(new Uint8Array())).toBeNull();
    expect(CityCatalog.parse(new Uint8Array([0x43, 0x54, 0x59, 0x31, 0, 0, 0, 0]))).toBeNull();
    expect(CityCatalog.parse(new Uint8Array([0x43, 0x54, 0x59, 0x31, 1, 0, 0, 0]))).toBeNull();
    const tampered = BYTES.slice();
    new DataView(tampered.buffer).setUint32(16, 0xffff_ffff, true);
    expect(CityCatalog.parse(tampered)).toBeNull();
  });

  it('has no nearest city for a coordinate that is not a point on Earth', () => {
    const cities = catalog();
    expect(cities.nearest(91, 0)).toBeNull();
    expect(cities.nearest(Number.NaN, 0)).toBeNull();
    expect(cities.nearest(0, Number.POSITIVE_INFINITY)).toBeNull();
  });
});

describe('great-circle distance', () => {
  it('has its exact quarter and antipodal values', () => {
    expect(distanceBetweenKm(0, 0, 0, 90)).toBeCloseTo((Math.PI / 2) * EARTH_MEAN_RADIUS_KM, 6);
    expect(distanceBetweenKm(0, 0, 0, 180)).toBeCloseTo(Math.PI * EARTH_MEAN_RADIUS_KM, 6);
  });

  it('crosses the antimeridian the short way', () => {
    expect(distanceBetweenKm(0, 179, 0, -179)).toBeLessThan(250);
  });
});
