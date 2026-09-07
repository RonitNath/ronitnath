import { expect } from '@playwright/test';

import { applyView, FOCAL, type Mat3 } from '../src/features/sky/sidereal';

/** The two pieces of geometry the sky specs share: where the page says the
 * observer is, and where a direction lands on screen. Both are asserted
 * against the modules that draw, never against a second implementation of
 * them. */

/** The observer, read back off the caption the page renders. */
export function observerFrom(caption: string): [number, number] {
  const match = /(\d+\.\d+)° ([NS]), (\d+\.\d+)° ([EW])/.exec(caption);
  expect(match, `no position in ${caption}`).not.toBeNull();
  const [, lat, ns, lon, ew] = match!;
  return [Number(lat) * (ns === 'S' ? -1 : 1), Number(lon) * (ew === 'W' ? -1 : 1)];
}

/** Where a J2000 direction lands on screen, by the projection the star pass
 * uses. `null` when it is below the horizon. */
export function project(
  matrix: Mat3,
  position: readonly number[],
  width: number,
  height: number,
): [number, number] | null {
  const [vx, vy, vz] = applyView(matrix, position);
  if (vz <= 0.2) return null;
  const aspect = width / height;
  return [
    (((vx / vz) * FOCAL) / aspect + 1) * 0.5 * width,
    (1 - (vy / vz) * FOCAL) * 0.5 * height,
  ];
}
