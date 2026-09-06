/** The travelling observer: a great circle over the rotating Earth.
 *
 * This is a route in Earth-*fixed* geographic coordinates, not an inertial
 * orbit. `viewMatrix` composes the longitude returned here with GMST; applying
 * Earth rotation in this module as well would double-count it.
 *
 * The circle is inclined 63 degrees, which is what makes it pass through San
 * Francisco while reaching a ±63 degree latitude band without dwelling near a
 * pole. One lap is exactly half a sidereal day of simulated time — an exact
 * divisor of the clock's resync period, so the observer's position is
 * continuous across that seam just as the sky's orientation is.
 */

import { SIM_EPOCH_MS } from './clock';
import {
  latLon,
  SF_LAT_DEG,
  SF_LON_DEG,
  SIDEREAL_DAY_MS,
  unitVector,
  type Vec3,
} from './sidereal';

export const TRACK_INCLINATION_DEG = 63;
export const TRACK_PERIOD_MS = SIDEREAL_DAY_MS / 2;

const DEG = Math.PI / 180;

function cross(a: Vec3, b: Vec3): Vec3 {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}

function normalize(v: Vec3): Vec3 {
  const norm = Math.hypot(v[0], v[1], v[2]);
  return [v[0] / norm, v[1] / norm, v[2] / norm];
}

/** Start point and the unit tangent at it — the two vectors that span the
 * great-circle plane. */
export function trackBasis(): [Vec3, Vec3] {
  const lat = SF_LAT_DEG * DEG;
  const lon = SF_LON_DEG * DEG;
  const start = unitVector(SF_LAT_DEG, SF_LON_DEG);

  // For inclination i the plane normal's z component is cos(i). Split the
  // horizontal part into local radial and east terms and constrain it to be
  // perpendicular to the start point.
  const inc = TRACK_INCLINATION_DEG * DEG;
  const normalZ = Math.cos(inc);
  const radial = -normalZ * Math.tan(lat);
  const east = Math.sqrt(Math.max(Math.sin(inc) ** 2 - radial ** 2, 0));
  const normal = normalize([
    radial * Math.cos(lon) - east * Math.sin(lon),
    radial * Math.sin(lon) + east * Math.cos(lon),
    normalZ,
  ]);
  return [start, normalize(cross(normal, start))];
}

/** The Earth-fixed observer subpoint at a simulation time, `[lat, lon]` in
 * degrees. At the simulation epoch the result is exactly San Francisco. */
export function observerAt(simMs: number): [number, number] {
  const elapsed =
    (((simMs - SIM_EPOCH_MS) % TRACK_PERIOD_MS) + TRACK_PERIOD_MS) % TRACK_PERIOD_MS;
  if (elapsed === 0) return [SF_LAT_DEG, SF_LON_DEG];
  const [start, tangent] = trackBasis();
  const phase = (2 * Math.PI * elapsed) / TRACK_PERIOD_MS;
  const [sin, cos] = [Math.sin(phase), Math.cos(phase)];
  return latLon([
    start[0] * cos + tangent[0] * sin,
    start[1] * cos + tangent[1] * sin,
    start[2] * cos + tangent[2] * sin,
  ]);
}

/** Shortest-path interpolation between two points on the sphere. Used when the
 * observer is moved by hand: a linear blend of latitude and longitude would
 * cut across the pole and jump the antimeridian. */
export function greatCircleLerp(
  a: readonly [number, number],
  b: readonly [number, number],
  t: number,
): [number, number] {
  const av = unitVector(a[0], a[1]);
  const bv = unitVector(b[0], b[1]);
  const dot = Math.min(1, Math.max(-1, av[0] * bv[0] + av[1] * bv[1] + av[2] * bv[2]));
  const angle = Math.acos(dot);
  if (angle < 1e-9) return [a[0], a[1]];
  const scale = Math.sin(angle);
  const wa = Math.sin((1 - t) * angle) / scale;
  const wb = Math.sin(t * angle) / scale;
  return latLon(
    normalize([av[0] * wa + bv[0] * wb, av[1] * wa + bv[1] * wb, av[2] * wa + bv[2] * wb]),
  );
}
