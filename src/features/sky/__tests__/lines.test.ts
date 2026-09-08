import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { CatalogError, parseStars } from '../catalog';
import { LINE_VERTEX_FLOATS, lineVertices, pairCount, parseLines, vertexCount } from '../lines';
import { SKY_ASSETS } from '../asset-names';
import { shipped } from './fixtures/shipped';

const BLOB = new Uint8Array(shipped(SKY_ASSETS.lines));
const MANIFEST = JSON.parse(readFileSync('public/sky/lines.json', 'utf8')) as {
  pairs: number;
  constellations: number;
  droppedPairs: number;
};
const STARS = parseStars(new Uint8Array(shipped(SKY_ASSETS.bright)));

function packed(pairs: readonly [number, number][]): Uint8Array {
  const bytes = new Uint8Array(8 + pairs.length * 4);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, 0x4e_49_4c_43, true);
  view.setUint32(4, pairs.length, true);
  pairs.forEach(([a, b], index) => {
    view.setUint16(8 + index * 4, a, true);
    view.setUint16(8 + index * 4 + 2, b, true);
  });
  return bytes;
}

describe('the packed constellation lines', () => {
  it('is the file the builder says it built', () => {
    const pairs = parseLines(BLOB);
    expect(pairCount(pairs)).toBe(MANIFEST.pairs);
    expect(MANIFEST.constellations).toBe(88);
    // Six of the figures' stars are not in bright.bin; the builder refuses
    // their segments rather than drawing a line to the wrong star.
    expect(MANIFEST.droppedPairs).toBeLessThan(20);
  });

  it('resolves every index into the star catalogue', () => {
    const pairs = parseLines(BLOB);
    for (const index of pairs) expect(index).toBeLessThan(STARS.count);
  });

  it('holds no segment twice and none from a star to itself', () => {
    const pairs = parseLines(BLOB);
    const seen = new Set<string>();
    for (let segment = 0; segment < pairCount(pairs); segment += 1) {
      const a = pairs[segment * 2]!;
      const b = pairs[segment * 2 + 1]!;
      expect(a).not.toBe(b);
      const key = `${Math.min(a, b)}-${Math.max(a, b)}`;
      expect(seen.has(key)).toBe(false);
      seen.add(key);
    }
  });

  it('refuses a file that is not one', () => {
    expect(() => parseLines(new Uint8Array(4))).toThrow(CatalogError);
    expect(() => parseLines(new Uint8Array(8))).toThrow(CatalogError);
    const short = packed([[0, 1]]).slice(0, 10);
    expect(() => parseLines(short)).toThrow(CatalogError);
    const wrongMagic = packed([[0, 1]]);
    wrongMagic[0] = 0;
    expect(() => parseLines(wrongMagic)).toThrow(CatalogError);
  });
});

describe('expanding the segments into a quad buffer', () => {
  it('gives six vertices a segment, each carrying both endpoints', () => {
    const data = lineVertices(STARS, parseLines(packed([[0, 1]])))!;
    expect(vertexCount(data)).toBe(6);
    for (let vertex = 0; vertex < 6; vertex += 1) {
      const at = vertex * LINE_VERTEX_FLOATS;
      expect(data[at]).toBeCloseTo(STARS.position[0]!, 6);
      expect(data[at + 3]).toBeCloseTo(STARS.position[3]!, 6);
      // The corner packs the side in its sign and the endpoint in its
      // magnitude, so it is never zero and never past two.
      expect(Math.abs(data[at + 6]!)).toBeGreaterThanOrEqual(1);
      expect(Math.abs(data[at + 6]!)).toBeLessThanOrEqual(2);
    }
    // Both sides of the line are used, and both endpoints.
    const corners = [...Array(6)].map((_, v) => data[v * LINE_VERTEX_FLOATS + 6]!);
    expect(new Set(corners).size).toBe(4);
  });

  it('drops a segment that points outside the catalogue rather than reading past it', () => {
    const outside = lineVertices(STARS, parseLines(packed([[0, 65_000]])));
    expect(outside).toBeNull();
  });

  it('expands the shipped file whole', () => {
    const pairs = parseLines(BLOB);
    const data = lineVertices(STARS, pairs)!;
    expect(vertexCount(data)).toBe(pairCount(pairs) * 6);
  });
});
