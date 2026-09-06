/** The mini-globe's geometry: orientation, picking, dragging, tessellation.
 *
 * The globe is an orthographic sphere oriented so the observer's subpoint is
 * always the point facing the viewer. That orientation is the whole design:
 * the globe is not a map with a dot on it, it is the same viewpoint the sky
 * above is drawn from, seen from outside.
 */

import { latLon, normalizeLonDeg, unitVector, type Vec3 } from './sidereal';

/** Fraction of the canvas half-width the sphere fills, leaving room for the
 * marker's halo at the limb. */
export const RADIUS = 0.92;

/** Sphere tessellation. 48×32 is under a thousand triangles and shows no
 * faceting at the size this is drawn. */
export const SEGMENTS: readonly [number, number] = [48, 32];

/** Degrees of rotation per pixel of drag. */
export const DRAG_SENSITIVITY = 0.45;

/** The world-to-view rotation that puts `(lat, lon)` at the centre of the disc
 * with north up: rows are east, north and up at that point. Column-major. */
export function orientation(latDeg: number, lonDeg: number): number[] {
  const DEG = Math.PI / 180;
  const [sinLat, cosLat] = [Math.sin(latDeg * DEG), Math.cos(latDeg * DEG)];
  const [sinLon, cosLon] = [Math.sin(lonDeg * DEG), Math.cos(lonDeg * DEG)];
  const rows = [
    [-sinLon, cosLon, 0],
    [-sinLat * cosLon, -sinLat * sinLon, cosLat],
    [cosLat * cosLon, cosLat * sinLon, sinLat],
  ];
  const out = new Array<number>(9);
  for (let row = 0; row < 3; row += 1) {
    for (let col = 0; col < 3; col += 1) out[col * 3 + row] = rows[row]![col]!;
  }
  return out;
}

/** Turn a point on the unit disc back into a point on Earth, given which point
 * the disc is currently centred on. `null` outside the sphere. */
export function unproject(
  x: number,
  y: number,
  centreLat: number,
  centreLon: number,
): [number, number] | null {
  const squared = x * x + y * y;
  if (!Number.isFinite(squared) || squared > 1) return null;
  const view = [x, y, Math.sqrt(1 - squared)] as const;
  const orient = orientation(centreLat, centreLon);
  // The basis is orthonormal, so its transpose is its inverse.
  const world: Vec3 = [0, 1, 2].map(
    (axis) =>
      orient[axis * 3]! * view[0] +
      orient[axis * 3 + 1]! * view[1] +
      orient[axis * 3 + 2]! * view[2],
  ) as unknown as Vec3;
  return latLon(world);
}

/** Where a drag of `(dx, dy)` pixels from `(lat, lon)` leaves the viewpoint.
 * Dragging right spins the Earth right, which moves the viewpoint west — the
 * globe behaves like an object under the hand, not like a scroll bar. */
export function dragTo(
  latDeg: number,
  lonDeg: number,
  dx: number,
  dy: number,
): [number, number] {
  const lat = latDeg + dy * DRAG_SENSITIVITY;
  return [
    Math.min(89.5, Math.max(-89.5, lat)),
    normalizeLonDeg(lonDeg - dx * DRAG_SENSITIVITY),
  ];
}

/** Interleaved `[x, y, z, u, v]` vertices and a triangle index list. */
export function sphere(
  lonSegments: number,
  latSegments: number,
): { vertices: Float32Array; indices: Uint16Array } {
  const vertices: number[] = [];
  for (let row = 0; row <= latSegments; row += 1) {
    const v = row / latSegments;
    const lat = 90 - v * 180;
    for (let column = 0; column <= lonSegments; column += 1) {
      const u = column / lonSegments;
      // The equirectangular texture starts at the antimeridian, which is where
      // lon = -180 puts u = 0.
      vertices.push(...unitVector(lat, u * 360 - 180), u, v);
    }
  }

  const indices: number[] = [];
  const stride = lonSegments + 1;
  for (let row = 0; row < latSegments; row += 1) {
    for (let column = 0; column < lonSegments; column += 1) {
      const a = row * stride + column;
      indices.push(a, a + stride, a + 1, a + 1, a + stride, a + stride + 1);
    }
  }
  return { vertices: new Float32Array(vertices), indices: new Uint16Array(indices) };
}
