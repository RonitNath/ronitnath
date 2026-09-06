import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { parseNamed } from '../catalog';
import { CityCatalog, type City } from '../cities';
import { SIM_EPOCH_MS, SPEED } from '../clock';
import {
  grounding,
  OVER_THRESHOLD_KM,
  placeText,
  positionText,
  starDetail,
  UPDATE_INTERVAL_MS,
} from '../label';
import { observerAt } from '../track';

const city = (distanceKm: number): City => ({ name: 'Testville', country: 'TS', distanceKm });

describe('the grounding caption', () => {
  it('rounds and groups distances the way a person reads them', () => {
    expect(placeText(city(1_403.8821))).toBe('1,404 km from Testville, TS');
    expect(placeText(city(12_345_678))).toBe('12,345,678 km from Testville, TS');
  });

  it('switches wording at the over threshold', () => {
    expect(placeText(city(OVER_THRESHOLD_KM - 0.01))).toBe('over Testville, TS');
    expect(placeText(city(OVER_THRESHOLD_KM))).toBe('50 km from Testville, TS');
  });

  it('names every quadrant and never prints a signed zero', () => {
    expect(positionText(37.7749, -122.4194)).toBe('37.77° N, 122.42° W');
    expect(positionText(-33.8688, 151.2093)).toBe('33.87° S, 151.21° E');
    expect(positionText(-0.001, -0.004)).toBe('0.00° N, 0.00° E');
  });

  it('leads with position and survives a catalog that never loaded', () => {
    const reykjavik: City = { name: 'Reykjavík', country: 'IS', distanceKm: 12 };
    expect(grounding(64.1, -21.8, reykjavik)).toBe('64.10° N, 21.80° W · over Reykjavík, IS');
    expect(grounding(64.1, -21.8, null)).toBe('64.10° N, 21.80° W');
  });

  it('actually changes between two consecutive refreshes', () => {
    // The point of the readout is that it is live. A regression that froze the
    // sample time would still render a plausible caption, and only this catches
    // it.
    const catalog = CityCatalog.parse(
      new Uint8Array(readFileSync('public/cities/cities.bin')),
    )!;
    const text = (simMs: number): string => {
      const [lat, lon] = observerAt(simMs);
      return grounding(lat, lon, catalog.nearest(lat, lon));
    };
    expect(text(SIM_EPOCH_MS)).not.toBe(text(SIM_EPOCH_MS + UPDATE_INTERVAL_MS * SPEED));
  });
});

describe('a callout detail line', () => {
  it('states the catalog and nothing more', () => {
    const star = parseNamed(readFileSync('public/stars/named.json', 'utf8')).stars[0]!;
    const detail = starDetail(star);
    expect(detail).toContain(star.constellation);
    expect(detail.endsWith(' ly')).toBe(true);
    expect(detail.split(' · ')).toHaveLength(3);
  });
});
