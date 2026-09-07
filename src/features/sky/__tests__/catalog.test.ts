import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import {
  CatalogError,
  HEADER_LEN,
  headerCount,
  KIND_GAIA,
  KIND_HIP,
  namedVectors,
  parseNamed,
  parseStars,
  starKey,
  starPosition,
  STRIDE,
} from '../catalog';

const BRIGHT = new Uint8Array(readFileSync('public/stars/bright.bin'));
const NAMED = readFileSync('public/stars/named.json', 'utf8');

/** One STR2 record, with everything but the arguments left valid. */
function oneRecord(x: number, y: number, z: number, kind = KIND_GAIA): Uint8Array {
  const bytes = new Uint8Array(HEADER_LEN + STRIDE);
  bytes.set([0x53, 0x54, 0x52, 0x32, 1, 0, 0, 0]);
  const view = new DataView(bytes.buffer);
  [x, y, z, 2].forEach((value, i) => view.setFloat32(HEADER_LEN + i * 4, value, true));
  bytes.set([255, 255, 255, 255], HEADER_LEN + 16);
  view.setBigUint64(HEADER_LEN + 20, 4_295_432_871_927_495_936n, true);
  view.setUint8(HEADER_LEN + 28, kind);
  return bytes;
}

const STR2_MAGIC = [0x53, 0x54, 0x52, 0x32];

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

  it('carries an id and the catalogue it belongs to', () => {
    const catalog = parseStars(BRIGHT);
    // Sirius is the brightest record and it comes from Hipparcos, because
    // Gaia saturates well below it (HIP 32349).
    expect(catalog.kind[0]).toBe(KIND_HIP);
    expect(catalog.id[0]).toBe(32_349n);
    expect(starKey(catalog, 0)).toBe('hip-32349');
    expect(starKey(catalog, catalog.count)).toBeNull();

    // The bulk of the file is Gaia, whose source_ids do not fit in a double.
    let gaia = 0;
    for (let index = 0; index < catalog.count; index += 1) {
      if (catalog.kind[index] === KIND_GAIA) gaia += 1;
    }
    expect(gaia).toBe(12_102);
    const last = catalog.count - 1;
    expect(catalog.id[last]).toBeGreaterThan(BigInt(Number.MAX_SAFE_INTEGER));
    expect(starKey(catalog, last)).toBe(`gaia-${catalog.id[last]}`);
  });

  it('has an id for every star and no id twice', () => {
    const catalog = parseStars(BRIGHT);
    const seen = new Set<string>();
    for (let index = 0; index < catalog.count; index += 1) {
      seen.add(starKey(catalog, index)!);
    }
    expect(seen.size).toBe(catalog.count);
  });

  it('reads a star position by index, bounded by the catalog', () => {
    const catalog = parseStars(BRIGHT);
    const vector = starPosition(catalog, 0)!;
    expect(Math.abs(Math.hypot(...vector) - 1)).toBeLessThan(0.01);
    expect(starPosition(catalog, 12_191)).toBeNull();
    expect(starPosition(catalog, -1)).toBeNull();
  });
});

/** Colour comes from a temperature: BP-RP (or B-V) -> Teff -> a blackbody
 * spectrum through the CIE observer -> sRGB, quantised onto 24 steps
 * (tools/starcat/colour.py). What has to hold of the shipped result is that it
 * is a *ramp* — no tiers the eye can find, nothing unreadable on near-black. */
describe('the star colour ramp', () => {
  const catalog = parseStars(BRIGHT);
  const levels = [...new Set(
    Array.from({ length: catalog.count }, (_, index) =>
      [0, 1, 2].map((c) => Math.round(catalog.color[index * 3 + c]! * 255)).join(','),
    ),
  )].map((key) => key.split(',').map(Number) as [number, number, number]);

  it('spends exactly the 24 levels it is allowed, and uses all of them', () => {
    expect(levels).toHaveLength(24);
  });

  it('is one monotonic blue-to-red ramp rather than a scatter of colours', () => {
    // Sorted from the coolest star to the hottest, red only ever falls and
    // blue only ever rises: a single family, not a palette.
    const ordered = [...levels].sort((a, b) => a[2] - a[0] - (b[2] - b[0]));
    for (let index = 1; index < ordered.length; index += 1) {
      expect(ordered[index]![0]).toBeLessThanOrEqual(ordered[index - 1]![0]);
      expect(ordered[index]![2]).toBeGreaterThanOrEqual(ordered[index - 1]![2]);
    }
  });

  it('steps too finely between levels for the eye to find a tier', () => {
    const ordered = [...levels].sort((a, b) => a[2] - a[0] - (b[2] - b[0]));
    for (let index = 1; index < ordered.length; index += 1) {
      for (let channel = 0; channel < 3; channel += 1) {
        const step = Math.abs(ordered[index]![channel]! - ordered[index - 1]![channel]!);
        expect(step).toBeLessThanOrEqual(16);
      }
    }
  });

  it('stays readable on a near-black page', () => {
    for (const [red, green, blue] of levels) {
      // Normalised: the catalog carries hue, and brightness is the renderer's
      // job, so every colour peaks at full scale on one channel...
      expect(Math.max(red, green, blue)).toBe(255);
      // ...and none of them has a channel dark enough to read as a hole.
      expect(Math.min(red, green, blue)).toBeGreaterThan(40);
    }
  });

  it('gives the stars whose colours are common knowledge the right ones', () => {
    const rgb = (index: number) =>
      [0, 1, 2].map((c) => Math.round(catalog.color[index * 3 + c]! * 255));
    const [betelgeuseR, , betelgeuseB] = rgb(9); // M2 supergiant: red
    expect(betelgeuseR).toBeGreaterThan(betelgeuseB! + 60);
    const [rigelR, , rigelB] = rgb(5); // B8 supergiant: blue-white
    expect(rigelB).toBeGreaterThan(rigelR! + 40);
    const [capellaR, , capellaB] = rgb(6); // G-type giants: near white
    expect(Math.abs(capellaR! - capellaB!)).toBeLessThan(60);
  });
});

describe('a refused star asset', () => {
  it('refuses a truncated or absurd header', () => {
    expect(() => headerCount(new Uint8Array([...STR2_MAGIC, 0, 0, 0, 0]))).toThrow(
      /out of bounds/,
    );
    expect(() => headerCount(new Uint8Array([...STR2_MAGIC, 255, 255, 255, 255]))).toThrow(
      /out of bounds/,
    );
    expect(() => headerCount(new Uint8Array([0x4e, 0x4f, 0x50, 0x45, 1, 0, 0, 0]))).toThrow(
      /bad star asset header/,
    );
    // STR1 is the format this replaced; a stale asset is refused, not read as
    // if its 20-byte records were 32-byte ones.
    expect(() => headerCount(new Uint8Array([0x53, 0x54, 0x52, 0x31, 1, 0, 0, 0]))).toThrow(
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

  it('refuses a record from a catalogue it cannot name', () => {
    expect(() => parseStars(oneRecord(1, 0, 0, KIND_HIP))).not.toThrow();
    expect(() => parseStars(oneRecord(1, 0, 0, 7))).toThrow(CatalogError);
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
