/** What a frame is asked to mark, computed away from the stage.
 *
 * Three questions the stage is asked every frame and answers from data it
 * merely holds: where the named stars are on screen, what colour a callout's
 * ring should be, and whether anything is still ringed. None of the three
 * needs the clock, the observer or the loop — they need a view matrix and a
 * catalogue — so they are here and `stage.ts` is the thirty lines that pass
 * them what they need.
 */

import {
  keepOutFor,
  place,
  placementAvoid,
  type Placement,
  separationFor,
} from './annotate';
import type { NamedStar, StarCatalog } from './catalog';
import type { Mat3, Vec3 } from './sidereal';
import type { Highlight } from './star-field';

/** How long a callout's ring stays on the star it points at. */
export const HIGHLIGHT_MS = 1_800;

/** A ring the stage is holding open until a moment in the future. */
export interface TimedHighlight extends Highlight {
  until: number;
}

/** Where the named stars are now, through the view the canvas last drew with.
 * The callout loop asks for this every frame and nothing about it goes through
 * React, which is what keeps the labels gliding rather than stepping. */
export function placementsFor(
  vectors: Vec3[],
  matrix: Mat3,
  aspect: number,
  width: number,
  height: number,
): Placement[] {
  if (!vectors.length) return [];
  return place(
    vectors,
    matrix,
    aspect,
    undefined,
    keepOutFor(width, height),
    placementAvoid(width, height),
    separationFor(width),
  );
}

/** The catalog colour of a named star: the callout's ring is drawn in it, so
 * the ring says which star as well as where. */
export function catalogColor(
  catalog: StarCatalog | null,
  named: NamedStar[],
  index: number,
): string | null {
  const star = named[index];
  if (!star || !catalog) return null;
  const at = star.brightIndex * 3;
  const byte = (offset: number): number =>
    Math.round((catalog.color[at + offset] ?? 1) * 255);
  return `rgb(${byte(0)},${byte(1)},${byte(2)})`;
}

/** The ring this frame draws: a pinned or hovered star outranks the fading one
 * a callout asked for, and a ring past its moment is gone. Returns the ring
 * and whether the timed one has expired, so the caller can drop it. */
export function highlightNow(
  ringed: { position: Vec3 } | null,
  timed: TimedHighlight | null,
  now: number,
): { highlight: Highlight | null; expired: boolean } {
  if (ringed) return { highlight: { position: ringed.position, strength: 1 }, expired: false };
  if (!timed) return { highlight: null, expired: false };
  const left = timed.until - now;
  if (left <= 0) return { highlight: null, expired: true };
  return { highlight: { position: timed.position, strength: left / HIGHLIGHT_MS }, expired: false };
}
