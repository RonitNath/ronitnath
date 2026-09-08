/** Where a tile is, and which tiles the view wants next.
 *
 * The sky is cut into 32 x 24 equal-angle cells (11.25 degrees of right
 * ascension by 7.5 of declination) and each is one static file. That grid is
 * not equal-*area* — a cell beside the pole is a sliver — but it is the grid
 * the builder cut, and every consequence of it lands here: a tile's angular
 * radius is measured from its own corners rather than assumed, so a polar
 * sliver is not treated as if it covered a degree it does not.
 *
 * The queue is the whole of S2's "view-first, orbit-ahead": score every tile
 * by how far its centre is from the zenith, subtract a bonus if it is inside
 * the frame at all, and add the tiles the orbit will be over in the next few
 * simulated minutes behind them.
 */

import { RA_BINS, DEC_BINS, TILE_COUNT } from './lod';
import { FOCAL, type Mat3, unitVector, type Vec3 } from './sidereal';

const RA_STEP = 360 / RA_BINS;
const DEC_STEP = 180 / DEC_BINS;

export function tileFor(raDeg: number, decDeg: number): number {
  const col = Math.min(RA_BINS - 1, Math.floor((((raDeg % 360) + 360) % 360) / RA_STEP));
  const row = Math.min(DEC_BINS - 1, Math.max(0, Math.floor((decDeg + 90) / DEC_STEP)));
  return row * RA_BINS + col;
}

/** `[raLo, raHi, decLo, decHi]` in degrees — the bounds the builder bucketed
 * on, so `tileFor` and this are inverses at the corners. */
export function tileBounds(id: number): [number, number, number, number] {
  const row = Math.floor(id / RA_BINS);
  const col = id % RA_BINS;
  return [col * RA_STEP, (col + 1) * RA_STEP, row * DEC_STEP - 90, (row + 1) * DEC_STEP - 90];
}

function centreOf(id: number): Vec3 {
  const [raLo, raHi, decLo, decHi] = tileBounds(id);
  return unitVector((decLo + decHi) / 2, (raLo + raHi) / 2);
}

/** How far a tile's own corners reach from its centre, in radians. Measured
 * once at module load for all 768: it is the number the frame test needs and
 * it never changes. */
function radiusOf(id: number, centre: Vec3): number {
  const [raLo, raHi, decLo, decHi] = tileBounds(id);
  let worst = 0;
  for (const ra of [raLo, raHi]) {
    for (const dec of [decLo, decHi]) {
      const corner = unitVector(dec, ra);
      const dot = centre[0] * corner[0] + centre[1] * corner[1] + centre[2] * corner[2];
      worst = Math.max(worst, Math.acos(Math.min(1, Math.max(-1, dot))));
    }
  }
  return worst;
}

export interface TileGeometry {
  readonly id: number;
  readonly centre: Vec3;
  /** Angular distance from the centre to the furthest corner. */
  readonly radius: number;
}

export const TILES: readonly TileGeometry[] = Array.from({ length: TILE_COUNT }, (_, id) => {
  const centre = centreOf(id);
  return { id, centre, radius: radiusOf(id, centre) };
});

export function angleBetween(a: Vec3, b: Vec3): number {
  const dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  return Math.acos(Math.min(1, Math.max(-1, dot)));
}

/** The zenith, in catalogue coordinates.
 *
 * The view matrix takes a J2000 direction into a frame whose z is the zenith,
 * and it is orthonormal, so the direction that arrives at (0, 0, 1) is its
 * third row — which in this column-major layout is elements 2, 5 and 8. */
export function zenithOf(matrix: Mat3): Vec3 {
  return [matrix[2]!, matrix[5]!, matrix[8]!];
}

/** Half the frame's diagonal field, in radians.
 *
 * The projection puts a direction at NDC `x = (vx/vz)·f/aspect`, `y =
 * (vy/vz)·f`, so the corner of the frame is the direction with
 * `tan(theta_x) = aspect/f` and `tan(theta_y) = 1/f`. At 16:9 that is 67.6
 * degrees from the zenith; at a phone's aspect it is nearer 62. It is derived
 * rather than written down because the whole queue is scored against it. */
export function frameRadiusRad(aspect: number): number {
  return Math.atan(Math.hypot(aspect / FOCAL, 1 / FOCAL));
}

export interface TileNeed {
  id: number;
  /** Angular distance, zenith to the tile's *near edge*, in radians: how far
   * from the zenith the deep sky can reach once this tile is resident. It is
   * negative for the tile the zenith is inside. */
  distance: number;
  /** Whether any part of the tile is inside the frame. */
  inFrame: boolean;
  /** Which orbit-ahead step asked for it, or -1 for none. */
  ahead: number;
  score: number;
}

/** A tile inside the frame outranks every tile outside it, whatever their
 * distances: `Math.PI` is longer than any angular distance can be, so
 * subtracting it partitions the two sets and leaves distance to order within
 * each. Cheaper and clearer than sorting on a tuple. */
const FRAME_BONUS = Math.PI;

/** What the view wants, best first.
 *
 * `zenith` and `ahead` are directions in catalogue coordinates: where the
 * observer is looking now, and where the same view maths says it will be
 * looking in +1, +2 and +3 simulated minutes. Tiles that are neither in the
 * frame nor under a future zenith are not candidates at all — there is no
 * reason to spend a byte on a tile behind the observer.
 */
export function rankTiles(
  zenith: Vec3,
  frameRadius: number,
  ahead: readonly Vec3[] = [],
): TileNeed[] {
  const needs: TileNeed[] = [];
  const aheadTile = new Map<number, number>();
  ahead.forEach((direction, step) => {
    const id = tileOf(direction);
    if (!aheadTile.has(id)) aheadTile.set(id, step);
  });

  for (const tile of TILES) {
    // Measured from the tile's near edge rather than its centre. On this
    // equal-angle grid a polar cell is a sliver and an equatorial one is
    // seven degrees across, so centre distance would rank a tile that adds
    // nothing to the covered cone above one that completes it — and the cone
    // is the whole of what the renderer can draw (`coveredRadiusRad`).
    const distance = angleBetween(zenith, tile.centre) - tile.radius;
    const inFrame = distance < frameRadius;
    const step = aheadTile.get(tile.id) ?? -1;
    if (!inFrame && step < 0) continue;
    needs.push({
      id: tile.id,
      distance,
      inFrame,
      ahead: step,
      // An orbit-ahead tile outside the frame sits after every tile in it,
      // ordered among its peers by how soon the orbit reaches it.
      score: inFrame ? distance - FRAME_BONUS : distance + step,
    });
  }
  needs.sort((a, b) => a.score - b.score);
  return needs;
}

/** The tile a direction falls in. */
export function tileOf(direction: Vec3): number {
  const dec = (Math.asin(Math.min(1, Math.max(-1, direction[2]))) * 180) / Math.PI;
  const ra = (Math.atan2(direction[1], direction[0]) * 180) / Math.PI;
  return tileFor(ra, dec);
}

/** The widest cone around the zenith inside which *every* tile is resident.
 *
 * This is what the deep pass is allowed to draw, and it is why there are never
 * tile edges on screen: a half-loaded neighbourhood is a rectangle of density,
 * so the renderer simply does not go there. The cone grows as tiles land,
 * which is the streaming the eye actually sees.
 */
export function coveredRadiusRad(
  zenith: Vec3,
  resident: ReadonlySet<number>,
  limit: number,
): number {
  let covered = limit;
  for (const tile of TILES) {
    if (resident.has(tile.id)) continue;
    const reach = angleBetween(zenith, tile.centre) - tile.radius;
    if (reach < covered) covered = Math.max(0, reach);
    if (covered === 0) break;
  }
  return covered;
}
