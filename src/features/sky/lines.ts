/** The constellation figures, as pairs of catalogue records.
 *
 * `sky/lines.bin` is 665 segments packed by `tools/starcat/build_lines.py`
 * from Stellarium's `constellationship.fab`, with every Hipparcos number
 * already resolved into an index in `bright.bin`. The browser therefore never
 * needs a Hipparcos catalogue, a name table or a lookup: a segment is two
 * numbers, and the two numbers are rows of a file it has already parsed.
 *
 *     b"CLIN"  u32 pair_count  (u16 a, u16 b) * pair_count
 *
 * Six of the figures' stars are not in `bright.bin` at all — five naked-eye
 * stars Gaia saturates on and one past the catalogue's G <= 6.5 cut — so
 * eleven of the 676 segments Stellarium draws are absent. The builder refuses
 * them rather than guessing, because a line to the wrong star is a wrong
 * constellation.
 */

import { CatalogError, type StarCatalog } from './catalog';

const MAGIC = 0x4e_49_4c_43; // "CLIN", little-endian.
const HEADER_LEN = 8;
const MAX_PAIRS = 20_000;

/** Two record indices per segment, flat: `[a0, b0, a1, b1, ...]`. */
export type LinePairs = Uint16Array;

/** Parse and check `lines.bin`. Nothing that arrived over the network is
 * trusted: an index past the catalogue would read another star's position out
 * of the buffer's tail, and a wrong length would silently truncate a figure. */
export function parseLines(bytes: Uint8Array): LinePairs {
  if (bytes.byteLength < HEADER_LEN) throw new CatalogError('bad line asset header');
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(0, true) !== MAGIC) throw new CatalogError('bad line asset header');
  const count = view.getUint32(4, true);
  if (count === 0 || count > MAX_PAIRS) throw new CatalogError('line count out of bounds');
  if (bytes.byteLength !== HEADER_LEN + count * 4) {
    throw new CatalogError('line asset length mismatch');
  }
  const pairs = new Uint16Array(count * 2);
  for (let index = 0; index < count * 2; index += 1) {
    pairs[index] = view.getUint16(HEADER_LEN + index * 2, true);
  }
  return pairs;
}

/** How many segments a pair list holds. */
export function pairCount(pairs: LinePairs): number {
  return pairs.length >> 1;
}

/** Six floats per vertex — the segment's two endpoints, repeated — and six
 * vertices per segment.
 *
 * The pass draws each segment as a quad rather than as a `LINES` primitive
 * because `lineWidth` is 1 device pixel and nothing else on every driver that
 * matters: at 2x DPR that is half a CSS pixel of hard-edged diagonal, which
 * against a star field reads as a dotted line. A quad carries both endpoints
 * to every one of its vertices so the vertex shader can work out the screen
 * direction itself, offset by the half-width, and let the fragment feather the
 * edge. {@link LINE_VERTEX_FLOATS} is that stride.
 */
export const LINE_VERTEX_FLOATS = 7;
const VERTICES_PER_SEGMENT = 6;
/** Which endpoint each of the six vertices belongs to, and which side of the
 * line it is offset to: two triangles, `a-b-b` then `a-b-a`. */
const CORNERS: readonly (readonly [number, number])[] = [
  [0, -1],
  [1, -1],
  [1, 1],
  [0, -1],
  [1, 1],
  [0, 1],
];

/** Expand the pairs into the vertex buffer, dropping any segment whose stars
 * are not both in this catalogue. Returns `null` when nothing survives. */
export function lineVertices(
  catalog: StarCatalog,
  pairs: LinePairs,
): Float32Array | null {
  const segments = pairCount(pairs);
  const data = new Float32Array(segments * VERTICES_PER_SEGMENT * LINE_VERTEX_FLOATS);
  let at = 0;
  for (let segment = 0; segment < segments; segment += 1) {
    const a = pairs[segment * 2]!;
    const b = pairs[segment * 2 + 1]!;
    if (a >= catalog.count || b >= catalog.count) continue;
    for (const [end, side] of CORNERS) {
      data[at] = catalog.position[a * 3]!;
      data[at + 1] = catalog.position[a * 3 + 1]!;
      data[at + 2] = catalog.position[a * 3 + 2]!;
      data[at + 3] = catalog.position[b * 3]!;
      data[at + 4] = catalog.position[b * 3 + 1]!;
      data[at + 5] = catalog.position[b * 3 + 2]!;
      // Packed into one float: the sign is the side, the magnitude the end.
      data[at + 6] = side * (end + 1);
      at += LINE_VERTEX_FLOATS;
    }
  }
  return at === 0 ? null : data.subarray(0, at);
}

/** How many vertices {@link lineVertices} produced. */
export function vertexCount(data: Float32Array): number {
  return data.length / LINE_VERTEX_FLOATS;
}
