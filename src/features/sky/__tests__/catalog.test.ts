import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import {
  CatalogError,
  HEADER_LEN,
  headerCount,
  namedVectors,
  parseNamed,
  parseStars,
  starPosition,
  STRIDE,
} from '../catalog';

const BRIGHT = new Uint8Array(readFileSync('public/stars/bright.bin'));
const NAMED = readFileSync('public/stars/named.json', 'utf8');

function oneRecord(x: number, y: number, z: number): Uint8Array {
  const bytes = new Uint8Array(HEADER_LEN + STRIDE);
  bytes.set([0x53, 0x54, 0x52, 0x31, 1, 0, 0, 0]);
  const view = new DataView(bytes.buffer);
  [x, y, z, 2].forEach((value, i) => view.setFloat32(HEADER_LEN + i * 4, value, true));
  bytes.set([255, 255, 255, 255], HEADER_LEN + 16);
  return bytes;
}

describe('the shipped star catalog', () => {
  it('validates and has its published count', () => {
    expect(parseStars(BRIGHT).count).toBe(12_191);
  });

  it('is sorted brightest first, so a partial read is still the sky', () => {
    const { magnitude } = parseStars(BRIGHT);
    for (let i = 1; i < magnitude.length; i += 1) {
      expect(magnitude[i]!).toBeGreaterThanOrEqual(magnitude[i - 1]!);
    }
  });

  it('reads a star position by index, bounded by the catalog', () => {
    const catalog = parseStars(BRIGHT);
    const vector = starPosition(catalog, 0)!;
    expect(Math.abs(Math.hypot(...vector) - 1)).toBeLessThan(0.01);
    expect(starPosition(catalog, 12_191)).toBeNull();
    expect(starPosition(catalog, -1)).toBeNull();
  });
});

describe('a refused star asset', () => {
  it('refuses a truncated or absurd header', () => {
    expect(() => headerCount(new Uint8Array([0x53, 0x54, 0x52, 0x31, 0, 0, 0, 0]))).toThrow(
      /out of bounds/,
    );
    expect(() =>
      headerCount(new Uint8Array([0x53, 0x54, 0x52, 0x31, 255, 255, 255, 255])),
    ).toThrow(/out of bounds/);
    expect(() => headerCount(new Uint8Array([0x4e, 0x4f, 0x50, 0x45, 1, 0, 0, 0]))).toThrow(
      /bad star asset header/,
    );
    expect(() => parseStars(oneRecord(1, 0, 0).subarray(0, HEADER_LEN + STRIDE - 1))).toThrow(
      /length mismatch/,
    );
  });

  it('refuses a record that is not a finite unit vector', () => {
    for (const bad of [oneRecord(0, 0, 0), oneRecord(Number.NaN, 0, 1), oneRecord(2, 0, 0)]) {
      expect(() => parseStars(bad)).toThrow(CatalogError);
    }
  });
});

describe('the named catalog', () => {
  it('is structured, attributed, and points inside the star catalog', () => {
    const catalog = parseNamed(NAMED);
    expect(catalog.version).toBe(1);
    expect(catalog.stars.length).toBeGreaterThanOrEqual(50);
    expect(catalog.sources.some((source) => source.includes('IAU'))).toBe(true);
    expect(catalog.sources.some((source) => source.includes('SIMBAD'))).toBe(true);
    const vectors = namedVectors(parseStars(BRIGHT), catalog);
    expect(vectors).toHaveLength(catalog.stars.length);
    for (const vector of vectors)
      expect(Math.abs(Math.hypot(...vector) - 1)).toBeLessThan(0.02);
  });

  it('refuses a catalog whose text is not a catalog', () => {
    expect(() => parseNamed('{"version":1,"stars":[]}')).toThrow(CatalogError);
    expect(() => parseNamed('{"version":1,"stars":[{"name":"x"}]}')).toThrow(CatalogError);
    expect(() =>
      parseNamed('{"version":1,"stars":[{"brightIndex":0,"name":"","constellation":"a"}]}'),
    ).toThrow(CatalogError);
  });

  it('refuses a named catalog that points outside the star catalog', () => {
    const stars = parseStars(BRIGHT);
    const named = parseNamed(
      '{"version":1,"sources":[],"stars":[{"brightIndex":999999,"name":"Nowhere",' +
        '"constellation":"None","classification":"none","distanceLy":1}]}',
    );
    expect(() => namedVectors(stars, named)).toThrow(/outside the star catalog/);
  });
});
