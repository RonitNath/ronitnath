/** The streamed Gaia catalogue's file format, and the colour it is drawn in.
 *
 * `tools/starcat/build_star_lod.py` writes two kinds of file into
 * `public/stars/lod/`: `g9.bin`, every G <= 9 star in one blob, and 768 tile
 * files, one per cell of a 32x24 equal-angle grid, carrying the rest down to
 * G = 12. Both are the same format — `GDR3LOD1`, a u32 count, then 16-byte
 * records — so one decoder reads both.
 *
 * A record is `<Qhhhh`: the Gaia source id, the direction *octahedrally*
 * encoded into two i16, the G magnitude in millimagnitudes, and BP-RP in
 * thousandths. The direction is two bytes per axis rather than three floats
 * because the file is the thing being streamed: 3 million records at 16 bytes
 * is 51 MB on disk and a third of a degree of angular error, which is a tenth
 * of a pixel at this focal length.
 *
 * Colour is *not* in the file the way the bright catalogue carries it (three
 * bytes of sRGB per star); the LOD record carries the observed colour index
 * and the same ramp is applied here. `tools/starcat/colour.py` is the
 * authority for that ramp — the 24 levels below are its output, so a G = 8
 * star from `g9.bin` and a G = 6 star from `bright.bin` of the same colour
 * arrive at the same three bytes.
 */

import type { Vec3 } from './sidereal';

export const LOD_MAGIC = 'GDR3LOD1';
/** Magic + u32 count. */
export const LOD_HEADER_LEN = 12;
export const LOD_RECORD_BYTES = 16;

/** The grid `build_star_lod.py` cut the sky on: 11.25 degrees of right
 * ascension by 7.5 of declination. */
export const RA_BINS = 32;
export const DEC_BINS = 24;
export const TILE_COUNT = RA_BINS * DEC_BINS;

/** Stars decoded for drawing, sorted brightest first.
 *
 * Brightest first is what makes a magnitude limit a *prefix*: the renderer
 * slides the limit with the device's pixel ratio and the theme, and sliding it
 * has to cost one number, not a pass over the buffer.
 *
 * Source ids are kept only when they are asked for. `g9.bin` is pickable — S3
 * hovers and clicks stars down to G = 9, and a pick has to be able to say
 * *which* star — so its 165k ids are worth 1.3 MB on the heap. The 768 tiles
 * are not pickable, and three million ids nothing reads would be 25 MB of
 * nothing.
 */
export interface DeepStars {
  count: number;
  /** Unit vectors, 3 per star. */
  position: Float32Array;
  /** G magnitude, ascending. */
  magnitude: Float32Array;
  /** sRGB, 3 bytes per star, from the ramp below. */
  color: Uint8Array;
  /** Gaia DR3 `source_id`, in the same order — only where the caller asked
   * for it (`parseDeep(bytes, true)`). */
  sourceId?: BigUint64Array;
}

/** The 24 colours `tools/starcat/colour.py` quantises a star's temperature
 * onto, coolest first: blackbody through the CIE 1931 observer to sRGB, then
 * the saturation and white-lift steps that module documents. Copied rather
 * than recomputed because it is a *shared* table — the bright catalogue has
 * these same bytes baked into it, and a second implementation of Planck's law
 * in TypeScript would be a second answer. */
export const COLOUR_RAMP: readonly (readonly [number, number, number])[] = [
  [255, 174, 87],
  [255, 178, 94],
  [255, 181, 102],
  [255, 185, 110],
  [255, 189, 118],
  [255, 194, 127],
  [255, 198, 136],
  [255, 203, 146],
  [255, 207, 156],
  [255, 212, 167],
  [255, 217, 178],
  [255, 222, 191],
  [255, 228, 204],
  [255, 233, 218],
  [255, 239, 232],
  [255, 245, 248],
  [246, 242, 255],
  [231, 233, 255],
  [217, 224, 255],
  [204, 216, 255],
  [193, 208, 255],
  [182, 201, 255],
  [173, 195, 255],
  [164, 189, 255],
];

const TEFF_MIN = 2_900;
const TEFF_MAX = 17_000;

/** Mucciarelli & Bellazzini 2020 (RNAAS 4, 52), the BP-RP dwarf fit at solar
 * metallicity: `5040/Teff = 0.4988 + 0.4925 C - 0.0287 C^2`. Clamped to the
 * range the ramp covers, exactly as `colour.py` clamps it. */
export function teffFromBpRp(bpRp: number): number {
  const c = Math.max(-0.4, Math.min(3.0, bpRp));
  return 5_040 / (0.4988 + 0.4925 * c - 0.0287 * c * c);
}

/** Which ramp level a temperature falls on. Even in 1/T, because colour moves
 * fast at the cool end and hardly at all above 10,000 K. */
export function colourLevel(teff: number): number {
  const clamped = Math.max(TEFF_MIN, Math.min(TEFF_MAX, teff));
  const t = (1 / clamped - 1 / TEFF_MIN) / (1 / TEFF_MAX - 1 / TEFF_MIN);
  return Math.max(0, Math.min(23, Math.round(t * 23)));
}

/** The inverse of the builder's `oct_encode`.
 *
 * Octahedral mapping: the unit sphere is projected onto the octahedron
 * `|x| + |y| + |z| = 1` and its lower half folded out into the unit square, so
 * two signed 16-bit numbers carry a direction to about 0.005 degrees. The fold
 * has to be undone in exactly the order the builder applied it or the southern
 * sky comes back mirrored — hence the sign dance below, and the test that
 * takes known right ascensions and declinations through both halves. */
export function octDecode(ox: number, oy: number): Vec3 {
  let x = ox / 32_767;
  let y = oy / 32_767;
  const z = 1 - Math.abs(x) - Math.abs(y);
  if (z < 0) {
    const sx = x >= 0 ? 1 : -1;
    const sy = y >= 0 ? 1 : -1;
    [x, y] = [(1 - Math.abs(y)) * sx, (1 - Math.abs(x)) * sy];
  }
  const norm = Math.hypot(x, y, z) || 1;
  return [x / norm, y / norm, z / norm];
}

/** Decode one `GDR3LOD1` file, or the front of one.
 *
 * The client asks for a `Range` of a large tile — the file is sorted brightest
 * first, so its first N records are the N brightest stars in that patch of sky
 * — and what comes back then carries a header counting records the body does
 * not have. That is the one length disagreement this accepts: a body of whole
 * records, no more than the header claims. Anything else is a truncated or
 * misrouted response and must not reach the GPU as noise. */
export function parseDeep(bytes: Uint8Array, keepIds = false): DeepStars {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let index = 0; index < 8; index += 1) {
    if (bytes[index] !== LOD_MAGIC.charCodeAt(index)) throw new Error('not a GDR3LOD1 file');
  }
  const declared = view.getUint32(8, true);
  const body = bytes.byteLength - LOD_HEADER_LEN;
  const count = Math.floor(body / LOD_RECORD_BYTES);
  if (body < 0 || body % LOD_RECORD_BYTES !== 0 || count > declared) {
    throw new Error(`GDR3LOD1 length ${bytes.byteLength} disagrees with ${declared} records`);
  }

  // Read magnitude and colour first, then walk the records in magnitude order:
  // one pass to sort an index, one to fill the buffers.
  const magnitude = new Float32Array(count);
  for (let index = 0; index < count; index += 1) {
    magnitude[index] =
      view.getInt16(LOD_HEADER_LEN + index * LOD_RECORD_BYTES + 12, true) / 1_000;
  }
  const order = new Uint32Array(count);
  for (let index = 0; index < count; index += 1) order[index] = index;
  order.sort((a, b) => magnitude[a]! - magnitude[b]!);

  const position = new Float32Array(count * 3);
  const color = new Uint8Array(count * 3);
  const sorted = new Float32Array(count);
  const sourceId = keepIds ? new BigUint64Array(count) : undefined;
  for (let slot = 0; slot < count; slot += 1) {
    const at = LOD_HEADER_LEN + order[slot]! * LOD_RECORD_BYTES;
    if (sourceId) sourceId[slot] = view.getBigUint64(at, true);
    const direction = octDecode(view.getInt16(at + 8, true), view.getInt16(at + 10, true));
    position[slot * 3] = direction[0];
    position[slot * 3 + 1] = direction[1];
    position[slot * 3 + 2] = direction[2];
    sorted[slot] = magnitude[order[slot]!]!;
    const rgb = COLOUR_RAMP[colourLevel(teffFromBpRp(view.getInt16(at + 14, true) / 1_000))]!;
    color[slot * 3] = rgb[0];
    color[slot * 3 + 1] = rgb[1];
    color[slot * 3 + 2] = rgb[2];
  }
  return { count, position, magnitude: sorted, color, sourceId };
}

/** How many of a magnitude-sorted array are at or below a limit. Binary
 * search: the renderer asks this of every resident tile every frame. */
export function countBrighterThan(magnitude: Float32Array, limit: number): number {
  let low = 0;
  let high = magnitude.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (magnitude[middle]! <= limit) low = middle + 1;
    else high = middle;
  }
  return low;
}

/** The manifest the builder writes beside the files. Only the fields the
 * client actually needs are typed; the sha256 per tile is the build's record,
 * not the browser's — a corrupt tile fails the length check above. */
export interface LodManifest {
  path: string;
  /** Whether each file's records are brightest first, which is what makes a
   * `Range` prefix of one a magnitude cut rather than a corner of the tile. */
  sortedByMagnitude?: boolean;
  raBins: number;
  decBins: number;
  gLimit: number;
  count: number;
  droppedBright: number;
  g9: { file: string; count: number; bytes: number };
  tiles: { id: number; file: string; count: number; bytes: number }[];
}

export function parseManifest(json: unknown): LodManifest {
  const manifest = json as LodManifest;
  if (
    !manifest ||
    manifest.raBins !== RA_BINS ||
    manifest.decBins !== DEC_BINS ||
    !Array.isArray(manifest.tiles) ||
    manifest.tiles.length !== TILE_COUNT ||
    !manifest.g9?.file
  ) {
    throw new Error('the LOD manifest does not describe this grid');
  }
  return manifest;
}

/** The faintest star the deep passes draw, by device pixel ratio.
 *
 * A G = 12 star lands a thousandth of the reference flux into a 0.6 px core:
 * at 2x it is a real fraction of one device pixel and reads as grain, at 1x
 * the same star is a quarter of that in a pixel four times the solid angle,
 * where it is below the display's step and only costs bandwidth and fill.
 * So the limit slides with the pixels there are to put it on. */
export function deepMagLimit(dpr: number): number {
  return dpr >= 1.5 ? 12 : 11;
}
