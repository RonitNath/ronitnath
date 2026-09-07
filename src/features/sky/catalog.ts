/** The two fetched star assets, and the validation that stands between the
 * network and the canvas.
 *
 * `bright.bin` is the catalog: an 8-byte `STR2` header and then one 32-byte
 * record per star — three f32 of unit position, one f32 magnitude, four bytes
 * of RGBA colour, a u64 catalogue id, and the byte that says which catalogue
 * that id belongs to — sorted brightest first, so a partial read is still the
 * brightest sky rather than a random subset of it.
 *
 * The id is what makes a star addressable: everything past this leg (streamed
 * depth, picking, a detail panel that can look a star up) needs to say *which*
 * star, and an index into a file that is rebuilt whenever the catalogue is
 * cannot say that. `tools/starcat/build_bright.py` writes the format.
 *
 * `named.json` is the annotation catalog: a small IAU/SIMBAD-attributed list
 * that points at records in `bright.bin` by index — and the builder refuses to
 * publish a catalog whose 50 named stars do not all still match by position.
 */

import type { Vec3 } from './sidereal';

export const HEADER_LEN = 8;
export const STRIDE = 32;
const MAGIC = 0x32_52_54_53; // "STR2", little-endian.
const MAX_STARS = 1_000_000;

/** Which catalogue a record's id belongs to. */
export const KIND_GAIA = 0;
export const KIND_HIP = 1;

export class CatalogError extends Error {}

/** Structure of arrays, because the canvas walks all of it every frame. */
export interface StarCatalog {
  /** J2000 unit vectors, three per star. */
  position: Float32Array;
  magnitude: Float32Array;
  /** RGB in 0..1, three per star. */
  color: Float32Array;
  /** Gaia DR3 `source_id` or Hipparcos number, one per star. */
  id: BigUint64Array;
  /** {@link KIND_GAIA} or {@link KIND_HIP}, one per star. */
  kind: Uint8Array;
  count: number;
}

/** Parse and check the header, returning the star count it declares. */
export function headerCount(bytes: Uint8Array): number {
  if (bytes.byteLength < HEADER_LEN) throw new CatalogError('bad star asset header');
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(0, true) !== MAGIC) throw new CatalogError('bad star asset header');
  const count = view.getUint32(4, true);
  if (count === 0 || count > MAX_STARS) throw new CatalogError('star count out of bounds');
  return count;
}

/** Full structural validation and unpacking: exact length, and every record a
 * finite unit vector. A star at the origin or with a NaN coordinate renders as
 * a full-screen artefact rather than as nothing, which is why this is checked
 * rather than trusted. */
export function parseStars(bytes: Uint8Array): StarCatalog {
  const count = headerCount(bytes);
  if (bytes.byteLength !== HEADER_LEN + count * STRIDE) {
    throw new CatalogError('star asset length mismatch');
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const position = new Float32Array(count * 3);
  const magnitude = new Float32Array(count);
  const color = new Float32Array(count * 3);
  const id = new BigUint64Array(count);
  const kind = new Uint8Array(count);

  for (let index = 0; index < count; index += 1) {
    const at = HEADER_LEN + index * STRIDE;
    const x = view.getFloat32(at, true);
    const y = view.getFloat32(at + 4, true);
    const z = view.getFloat32(at + 8, true);
    const mag = view.getFloat32(at + 12, true);
    const normSquared = x * x + y * y + z * z;
    if (!Number.isFinite(mag) || !(normSquared >= 0.98 && normSquared <= 1.02)) {
      throw new CatalogError('invalid star record');
    }
    position[index * 3] = x;
    position[index * 3 + 1] = y;
    position[index * 3 + 2] = z;
    magnitude[index] = mag;
    color[index * 3] = view.getUint8(at + 16) / 255;
    color[index * 3 + 1] = view.getUint8(at + 17) / 255;
    color[index * 3 + 2] = view.getUint8(at + 18) / 255;
    id[index] = view.getBigUint64(at + 20, true);
    const which = view.getUint8(at + 28);
    if (which !== KIND_GAIA && which !== KIND_HIP) {
      throw new CatalogError('invalid star record');
    }
    kind[index] = which;
  }
  return { position, magnitude, color, id, kind, count };
}

/** How a star names itself: the form `/api/sky/star/<key>` will read, and the
 * form a callout or a pick shows when the star has no proper name. */
export function starKey(catalog: StarCatalog, index: number): string | null {
  if (!Number.isInteger(index) || index < 0 || index >= catalog.count) return null;
  const prefix = catalog.kind[index] === KIND_HIP ? 'hip' : 'gaia';
  return `${prefix}-${catalog.id[index]!}`;
}

/** The J2000 unit vector of the star at `index`. */
export function starPosition(catalog: StarCatalog, index: number): Vec3 | null {
  if (!Number.isInteger(index) || index < 0 || index >= catalog.count) return null;
  return [
    catalog.position[index * 3]!,
    catalog.position[index * 3 + 1]!,
    catalog.position[index * 3 + 2]!,
  ];
}

export interface NamedStar {
  brightIndex: number;
  name: string;
  constellation: string;
  classification: string;
  distanceLy: number;
}

export interface NamedCatalog {
  version: number;
  sources: string[];
  stars: NamedStar[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

/** Parse `named.json`. Nothing about a catalog that arrived over the network
 * is trusted: a name that is not a string would end up in the DOM, and an
 * index that is not one would label the wrong star. */
export function parseNamed(json: string): NamedCatalog {
  const parsed: unknown = JSON.parse(json);
  if (!isRecord(parsed) || typeof parsed.version !== 'number') {
    throw new CatalogError('named.json is not a catalog');
  }
  const sources = Array.isArray(parsed.sources)
    ? parsed.sources.filter((source): source is string => typeof source === 'string')
    : [];
  if (!Array.isArray(parsed.stars) || parsed.stars.length === 0) {
    throw new CatalogError('named.json has no stars');
  }
  const stars = parsed.stars.map((entry): NamedStar => {
    if (
      !isRecord(entry) ||
      !Number.isInteger(entry.brightIndex) ||
      typeof entry.name !== 'string' ||
      entry.name === '' ||
      typeof entry.constellation !== 'string' ||
      entry.constellation === '' ||
      typeof entry.classification !== 'string' ||
      entry.classification === '' ||
      typeof entry.distanceLy !== 'number' ||
      !Number.isFinite(entry.distanceLy) ||
      entry.distanceLy <= 0
    ) {
      throw new CatalogError('named.json holds a malformed star');
    }
    return {
      brightIndex: entry.brightIndex as number,
      name: entry.name,
      constellation: entry.constellation,
      classification: entry.classification,
      distanceLy: entry.distanceLy,
    };
  });
  return { version: parsed.version, sources, stars };
}

/** The J2000 vectors the named catalog points at. A catalog whose indices do
 * not all resolve is a mismatched pair of assets, and labelling the wrong
 * stars is worse than labelling none. */
export function namedVectors(catalog: StarCatalog, named: NamedCatalog): Vec3[] {
  return named.stars.map((star) => {
    const position = starPosition(catalog, star.brightIndex);
    if (!position) throw new CatalogError('named.json points outside the star catalog');
    return position;
  });
}
