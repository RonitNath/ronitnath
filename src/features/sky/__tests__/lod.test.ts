import { describe, expect, it } from 'vitest';

import {
  colourLevel,
  COLOUR_RAMP,
  countBrighterThan,
  deepMagLimit,
  LOD_HEADER_LEN,
  LOD_RECORD_BYTES,
  octDecode,
  parseDeep,
  parseManifest,
  teffFromBpRp,
} from '../lod';
import { tileFor } from '../lod-tiles';
import { unitVector } from '../sidereal';
import { SKY_ASSETS } from '../asset-names';
import { shippedLod, shippedText } from './fixtures/shipped';

/** The builder's `oct_encode`, transcribed from `tools/starcat/build_star_lod.py`.
 * The decoder is asserted against *this* rather than against a round trip
 * through itself, which would agree with any consistent mistake — a mirrored
 * southern sky included. */
function encodeExact(raDeg: number, decDeg: number): [number, number] {
  const [ra, dec] = [(raDeg * Math.PI) / 180, (decDeg * Math.PI) / 180];
  const v = [Math.cos(dec) * Math.cos(ra), Math.cos(dec) * Math.sin(ra), Math.sin(dec)];
  const scale = Math.abs(v[0]!) + Math.abs(v[1]!) + Math.abs(v[2]!);
  let [x, y] = [v[0]! / scale, v[1]! / scale];
  const z = v[2]! / scale;
  if (z < 0) {
    [x, y] = [(1 - Math.abs(y)) * (x >= 0 ? 1 : -1), (1 - Math.abs(x)) * (y >= 0 ? 1 : -1)];
  }
  return [Math.round(x * 32_767), Math.round(y * 32_767)];
}

function fileOf(records: { ra: number; dec: number; mag: number; bpRp: number }[]): Uint8Array {
  const bytes = new Uint8Array(LOD_HEADER_LEN + records.length * LOD_RECORD_BYTES);
  bytes.set([...'GDR3LOD1'].map((c) => c.charCodeAt(0)));
  const view = new DataView(bytes.buffer);
  view.setUint32(8, records.length, true);
  records.forEach((record, index) => {
    const at = LOD_HEADER_LEN + index * LOD_RECORD_BYTES;
    view.setBigUint64(at, BigInt(index + 1), true);
    const [ox, oy] = encodeExact(record.ra, record.dec);
    view.setInt16(at + 8, ox, true);
    view.setInt16(at + 10, oy, true);
    view.setInt16(at + 12, Math.round(record.mag * 1_000), true);
    view.setInt16(at + 14, Math.round(record.bpRp * 1_000), true);
  });
  return bytes;
}

/** A degree of arc, in the dot-product terms the tests assert in. */
const ARCSEC = Math.PI / (180 * 3_600);

describe('the octahedral direction', () => {
  const places: [string, number, number][] = [
    ['the vernal equinox', 0, 0],
    ['Sirius', 101.287, -16.716],
    ['Vega', 279.234, 38.784],
    ['the north celestial pole', 0, 89.999],
    ['the south celestial pole', 123, -89.999],
    ['deep in the southern fold', 200.5, -60.25],
    ['just below the equator', 359.9, -0.01],
  ];

  it.each(places)('survives the fold at %s', (_name, ra, dec) => {
    const [ox, oy] = encodeExact(ra, dec);
    const decoded = octDecode(ox, oy);
    const truth = unitVector(dec, ra);
    const dot = decoded[0] * truth[0] + decoded[1] * truth[1] + decoded[2] * truth[2];
    // 16 bits over an octahedron is about 20 arcseconds at worst.
    expect(Math.acos(Math.min(1, dot))).toBeLessThan(30 * ARCSEC);
  });

  it('folds the south rather than projecting it', () => {
    // The fold is the whole subtlety of the encoding: a southern direction
    // must not land where its unfolded projection would, or half the sky
    // comes back mirrored onto the other half.
    const [ra, dec] = [200.5, -60.25];
    const v = unitVector(dec, ra);
    const scale = Math.abs(v[0]) + Math.abs(v[1]) + Math.abs(v[2]);
    const unfolded = [Math.round((v[0] / scale) * 32_767), Math.round((v[1] / scale) * 32_767)];
    expect(encodeExact(ra, dec)).not.toEqual(unfolded);
    expect(octDecode(...(encodeExact(ra, dec) as [number, number]))[2]).toBeLessThan(0);
  });
});

describe('parseDeep', () => {
  const records = [
    { ra: 10, dec: 20, mag: 11.5, bpRp: 1.4 },
    { ra: 190, dec: -35, mag: 8.25, bpRp: 0.1 },
    { ra: 300, dec: 70, mag: 9.75, bpRp: 2.6 },
  ];

  it('decodes every record and sorts them brightest first', () => {
    const stars = parseDeep(fileOf(records));
    expect(stars.count).toBe(3);
    expect([...stars.magnitude]).toEqual([8.25, 9.75, 11.5]);
    const truth = unitVector(-35, 190);
    for (let axis = 0; axis < 3; axis += 1) {
      expect(stars.position[axis]).toBeCloseTo(truth[axis]!, 3);
    }
  });

  it('gives a star the colour its temperature earns', () => {
    const stars = parseDeep(fileOf(records));
    // Sorted brightest first, the middle record is the BP-RP 2.6 star: the
    // red end of the ramp, and redder than the BP-RP 0.1 one before it.
    const red = [...stars.color.slice(3, 6)];
    const blue = [...stars.color.slice(0, 3)];
    expect(red).toEqual([...COLOUR_RAMP[colourLevel(teffFromBpRp(2.6))]!]);
    expect(red[2]!).toBeLessThan(blue[2]!);
  });

  it('takes the front of a file as the front of a file', () => {
    // What a `Range` request comes back as: the header still counts three
    // records and only one arrived, which is the case the client relies on.
    const prefix = fileOf(records).slice(0, LOD_HEADER_LEN + LOD_RECORD_BYTES);
    expect(parseDeep(prefix).count).toBe(1);
  });

  it('refuses a file that is not one', () => {
    expect(() => parseDeep(new Uint8Array(20))).toThrow(/GDR3LOD1/);
    // Half a record is a broken response, not a shorter one...
    const ragged = fileOf(records).slice(0, LOD_HEADER_LEN + LOD_RECORD_BYTES + 7);
    expect(() => parseDeep(ragged)).toThrow(/disagrees/);
    // ...and so is a body with more records than the header admits to.
    const overlong = new Uint8Array(LOD_HEADER_LEN + 4 * LOD_RECORD_BYTES);
    overlong.set(fileOf(records));
    expect(() => parseDeep(overlong)).toThrow(/disagrees/);
  });
});

describe('the shipped assets', () => {
  const manifest = parseManifest(
    JSON.parse(shippedText(SKY_ASSETS.lodManifest)),
  );

  it('says the bright catalogue was deduplicated at build', () => {
    // S2 does no runtime dedupe, and this is why: the builder dropped every
    // record the STR2 catalogue already carries, so nothing can be drawn
    // twice. If this ever came back zero, the client would need to.
    expect(manifest.droppedBright).toBeGreaterThan(10_000);
    expect(manifest.count).toBe(manifest.tiles.reduce((total, tile) => total + tile.count, 0));
  });

  it('decodes a real tile into its own patch of sky', () => {
    const tile = manifest.tiles[300]!;
    const stars = parseDeep(shippedLod(tile.file));
    expect(stars.count).toBe(tile.count);
    for (let index = 0; index < stars.count; index += 97) {
      const at = index * 3;
      const [x, y, z] = [stars.position[at]!, stars.position[at + 1]!, stars.position[at + 2]!];
      expect(Math.hypot(x, y, z)).toBeCloseTo(1, 3);
      const dec = (Math.asin(z) * 180) / Math.PI;
      const ra = (Math.atan2(y, x) * 180) / Math.PI;
      expect(tileFor(ra, dec)).toBe(300);
      expect(stars.magnitude[index]).toBeLessThanOrEqual(manifest.gLimit);
    }
  });

  it('is sorted brightest first, which is what a Range prefix relies on', () => {
    const tile = manifest.tiles[280]!;
    expect(manifest.sortedByMagnitude).toBe(true);
    const whole = shippedLod(tile.file);
    const stars = parseDeep(whole);
    expect(stars.count).toBeGreaterThan(4_096);
    // The first 4,096 records of the file are the 4,096 brightest stars in
    // that patch of sky — a magnitude cut, so spatially even.
    const prefix = parseDeep(whole.subarray(0, LOD_HEADER_LEN + 4_096 * LOD_RECORD_BYTES));
    expect(prefix.count).toBe(4_096);
    expect([...prefix.magnitude]).toEqual([...stars.magnitude.slice(0, 4_096)]);
    expect(prefix.magnitude.at(-1)!).toBeLessThanOrEqual(stars.magnitude[4_096]!);
  });

  it('rejects a manifest cut on another grid', () => {
    expect(() => parseManifest({ ...manifest, raBins: 16 })).toThrow(/grid/);
  });
});

describe('the magnitude limit', () => {
  it('is a prefix of the sorted buffer', () => {
    const magnitude = Float32Array.from([6, 7.5, 9, 10.5, 12]);
    expect(countBrighterThan(magnitude, 9)).toBe(3);
    expect(countBrighterThan(magnitude, 5)).toBe(0);
    expect(countBrighterThan(magnitude, 99)).toBe(5);
  });

  it('slides with the pixels there are to draw on', () => {
    expect(deepMagLimit(2)).toBeGreaterThan(deepMagLimit(1));
  });
});
