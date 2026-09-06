/** Placing the named-star callouts.
 *
 * A callout is only worth drawing if the star it names is actually in the
 * frame, so this projects the named catalog through the same view matrix the
 * canvas draws with and keeps the ones that land inside a margin. When nothing
 * clears that margin — which happens at the narrow end of a phone viewport —
 * the best above-horizon star is *forced* to the edge rather than the sky going
 * unlabelled, and it says so, because a clamped position is not a measurement
 * of where the star is.
 */

import { applyView, FOCAL, type Mat3, type Vec3 } from './sidereal';

export interface Placement {
  /** Index into the named catalog that was passed in. */
  star: number;
  /** The star's J2000 unit vector, carried here so a click needs no second
   * lookup into the catalog. */
  position: Vec3;
  /** -1..1, left to right. */
  x: number;
  /** -1..1, bottom to top. */
  y: number;
  /** The star is above the horizon but outside the comfortable margin, and
   * this position has been clamped to the edge to keep a label on screen. */
  forced: boolean;
}

/** Inside this fraction of the frame a callout has room for its two lines
 * without colliding with the edge. */
const MARGIN_X = 0.88;
const MARGIN_Y = 0.82;

/** Stars below this much of the zenith cosine are too near the horizon: the
 * projection stretches without bound there and the label would slide off. */
const MIN_ZENITH_COS = 0.12;

/** The hero card and the widest callout the named catalog produces
 * ("Orion · blue supergiant · 863 ly"), measured on the shipped landing. */
export const HERO_PX: readonly [number, number] = [368.72, 135.39];
export const LABEL_PX: readonly [number, number] = [328.13, 36.47];

/** The keep-out band at 1440×900, and the headroom the shipped numbers carry
 * over the bare measurement. */
export const KEEP_OUT: readonly [number, number] = [0.52, 0.24];
const HEADROOM: readonly [number, number] = [
  KEEP_OUT[0] / ((HERO_PX[0] + LABEL_PX[0]) / 1440),
  KEEP_OUT[1] / ((HERO_PX[1] + LABEL_PX[1]) / 900),
];

/** The hero sits in the middle of the frame, and a label behind it is not a
 * label — it is two texts on top of each other. The band is the hero's box
 * **grown by half a label**, because a placement is where the label is centred
 * and not where it ends: a band that only cleared the card itself kept the
 * *point* off the hero and let the text sit on it anyway.
 *
 * It is measured against the viewport rather than fixed, because the same card
 * is nearly the full width of a phone and barely a quarter of a desktop: the
 * desktop number reads as a band beside the hero and, held constant, reads as
 * no band at all at 390 px. Across, it stops just short of the margin a forced
 * placement is clamped to, so that placement is outside the band by
 * construction and the sky is never left unlabelled. */
export function keepOutFor(width: number, height: number): [number, number] {
  const hero = Math.min(HERO_PX[0], Math.max(0, width - 32));
  const label = Math.min(LABEL_PX[0], Math.max(0, width - 24));
  return [
    Math.min(MARGIN_X - 0.02, ((hero + label) / width) * HEADROOM[0]),
    Math.min(0.6, ((HERO_PX[1] + LABEL_PX[1]) / height) * HEADROOM[1]),
  ];
}

/** Two callouts stacked within this much of each other in normalised device
 * coordinates are one unreadable block of text, not two labels. */
const MIN_SEPARATION: readonly [number, number] = [0.55, 0.14];

/** How many labels the frame carries at once. Three is what fits down one side
 * of a phone without the sky becoming a list. */
export const MAX_LABELS = 3;

function clamp(value: number, limit: number): number {
  return value < -limit ? -limit : value > limit ? limit : value;
}

/** Project `stars` (J2000 unit vectors) and return up to `limit` placements,
 * brightest-first in catalog order. */
export function place(
  stars: readonly Vec3[],
  matrix: Mat3,
  aspect: number,
  limit: number = MAX_LABELS,
  keepOut: readonly [number, number] = KEEP_OUT,
): Placement[] {
  const inside: Placement[] = [];
  let bestForced: Placement | null = null;

  for (let star = 0; star < stars.length; star += 1) {
    const vector = stars[star]!;
    const [vx, vy, vz] = applyView(matrix, vector);
    if (vz <= MIN_ZENITH_COS) continue;

    const x = ((vx / vz) * FOCAL) / aspect;
    const y = (vy / vz) * FOCAL;
    if (Math.abs(x) < keepOut[0] && Math.abs(y) < keepOut[1]) continue;

    if (Math.abs(x) <= MARGIN_X && Math.abs(y) <= MARGIN_Y) {
      const collides = inside.some(
        (other) =>
          Math.abs(other.x - x) < MIN_SEPARATION[0] &&
          Math.abs(other.y - y) < MIN_SEPARATION[1],
      );
      if (collides) continue;
      inside.push({ star, position: vector, x, y, forced: false });
      if (inside.length === limit) return inside;
    } else if (!bestForced) {
      bestForced = {
        star,
        position: vector,
        x: clamp(x, MARGIN_X),
        y: clamp(y, MARGIN_Y),
        forced: true,
      };
    }
  }

  if (inside.length === 0) return bestForced ? [bestForced] : [];
  return inside;
}

/** Normalised device coordinates as CSS percentages of the viewport. The y
 * axis flips: NDC counts up from the bottom, CSS counts down from the top. */
export function cssPosition(placement: Placement): [number, number] {
  return [(placement.x + 1) * 50, (1 - placement.y) * 50];
}

/** A placement's sky direction, as the three numbers a click carries. */
export function positionAttribute(placement: Placement): string {
  return placement.position.join(',');
}

/** Read a position back off a label. Anything that is not three finite numbers
 * is no position at all. */
export function parsePosition(value: string): Vec3 | null {
  const parts = value.split(',');
  if (parts.length !== 3) return null;
  const numbers = parts.map((part) => Number(part.trim()));
  if (!numbers.every((number) => Number.isFinite(number))) return null;
  const [x, y, z] = numbers as [number, number, number];
  return Math.hypot(x, y, z) > 0.5 ? [x, y, z] : null;
}
