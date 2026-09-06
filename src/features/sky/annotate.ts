/** Placing the named-star callouts.
 *
 * A callout is only worth drawing if the star it names is actually in the
 * frame, so this projects the named catalog through the same view matrix the
 * canvas draws with and keeps the ones that land inside a margin. When nothing
 * clears that margin — which happens at the narrow end of a phone viewport —
 * the best above-horizon star is *forced* to the edge rather than the sky going
 * unlabelled, and it says so, because a clamped position is not a measurement
 * of where the star is.
 *
 * A callout is not drawn *on* its star — it is offset from it with a line back
 * to a ring around it, so the name never sits on the thing it names. Which way
 * it is offset is chosen here too, because the answer depends on the same two
 * facts the placement does: where the hero band is and where the frame ends.
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
export const HERO_PX: readonly [number, number] = [370.33, 145.39];
export const LABEL_PX: readonly [number, number] = [328.13, 36.47];

/** The same card once the four social links wrap to two rows: narrower,
 * because it shrink-wraps its content, and sixty pixels taller. A band that
 * used the desktop height left the bottom of a phone's card outside it, and a
 * callout placed just under the band sat on the last row of links. */
export const HERO_WRAPPED_PX: readonly [number, number] = [208.81, 204.39];

/** The width the links wrap at, between a phone and the narrowest tablet. */
const HERO_WRAP_WIDTH = 480;

export function heroFor(width: number): readonly [number, number] {
  return width <= HERO_WRAP_WIDTH ? HERO_WRAPPED_PX : HERO_PX;
}

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
 * no band at all at 390 px. Across, it reaches the whole placeable width on a
 * phone — which is the truth there, since one label is wider than the frame
 * beside the card — so it is the *vertical* rule that places a phone's labels,
 * above the hero or below it. A sliver left between the band and the margin is
 * what put a callout on the name: a placement inside it is legal and its label
 * still runs straight across the card, because no direction can escape a band
 * that is wider than the label is long. */
export function keepOutFor(width: number, height: number): [number, number] {
  const [heroWidth, heroHeight] = heroFor(width);
  const hero = Math.min(heroWidth, Math.max(0, width - 32));
  const label = Math.min(LABEL_PX[0], Math.max(0, width - 24));
  return [
    Math.min(MARGIN_X, ((hero + label) / width) * HEADROOM[0]),
    Math.min(0.6, ((heroHeight + LABEL_PX[1]) / height) * HEADROOM[1]),
  ];
}

/** How far above the keep-out band a forced placement is pushed. A forced
 * label is as wide as the phone it is on, so its point clearing the band
 * across is not enough — the text lies straight over the name. Pushing the
 * clamped y clear of the band is what actually keeps them apart. */
const BAND_CLEARANCE = 0.12;

/** A clamped y moved out of the hero band: up if the star is in the upper
 * half, down if it is in the lower, and never past the frame margin. */
export function pushOutOfBand(y: number, keepOutY: number): number {
  const wall = Math.min(MARGIN_Y, keepOutY + BAND_CLEARANCE);
  if (Math.abs(y) >= wall) return y;
  return y >= 0 ? wall : -wall;
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
    const onHero = Math.abs(x) < keepOut[0] && Math.abs(y) < keepOut[1];

    if (!onHero && Math.abs(x) <= MARGIN_X && Math.abs(y) <= MARGIN_Y) {
      const collides = inside.some(
        (other) =>
          Math.abs(other.x - x) < MIN_SEPARATION[0] &&
          Math.abs(other.y - y) < MIN_SEPARATION[1],
      );
      if (collides) continue;
      inside.push({ star, position: vector, x, y, forced: false });
      if (inside.length === limit) return inside;
    } else if (!bestForced) {
      // The brightest star that could not be labelled where it is. On a phone
      // that is most of the sky — the band across is the whole placeable frame
      // — so the fallback has to accept a star *on* the hero too, and move its
      // label off it, or the sky goes unlabelled on the viewport that most
      // needs the one line of context.
      bestForced = {
        star,
        position: vector,
        x: clamp(x, MARGIN_X),
        y: pushOutOfBand(clamp(y, MARGIN_Y), keepOut[1]),
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

/* ------------------------------------------------------------- leader ---- */

/** How far the label sits from its star. A phone gets less, because the same
 * gap is a larger share of a 390 px frame and pushes the text into the edge
 * the placement just cleared. */
export const LEADER_OFFSET_PX = 36;
export const LEADER_OFFSET_PHONE_PX = 26;
export const PHONE_MAX_WIDTH_PX = 640;

/** The ring drawn around the star the line runs back to. */
export const RING_RADIUS_PX = 3;

/** The leader's own drawing surface, centred on the star. A zero-sized `svg`
 * keeps its geometry but paints nothing outside its viewport, whatever
 * `overflow` says, so the overlay is given a real box and a `viewBox` that
 * puts the origin — the star — at its centre. */
export const LEADER_SVG_PX = 128;

export function leaderOffset(width: number): number {
  return width <= PHONE_MAX_WIDTH_PX ? LEADER_OFFSET_PHONE_PX : LEADER_OFFSET_PX;
}

export type LeaderName = 'up-right' | 'up-left' | 'down-right' | 'down-left';

/** Preference order. Up-and-out first: a label above its star reads as a
 * caption of it, and the sky is emptier above than below, where the globe and
 * the caption live. */
export const LEADER_ORDER: readonly LeaderName[] = [
  'up-right',
  'up-left',
  'down-right',
  'down-left',
];

/** Unit directions in CSS pixels, where y counts down the frame. */
const LEADER_VECTOR: Record<LeaderName, readonly [number, number]> = {
  'up-right': [Math.SQRT1_2, -Math.SQRT1_2],
  'up-left': [-Math.SQRT1_2, -Math.SQRT1_2],
  'down-right': [Math.SQRT1_2, Math.SQRT1_2],
  'down-left': [-Math.SQRT1_2, Math.SQRT1_2],
};

export interface LeaderFrame {
  /** Viewport, CSS pixels. */
  width: number;
  height: number;
  /** The label's box, CSS pixels. */
  label: readonly [number, number];
  /** The hero band, as NDC half-extents — what {@link keepOutFor} returns. */
  keepOut: readonly [number, number];
}

/** The margin a label keeps from the frame edge. */
export const EDGE_PX = 8;

/** The widest a label can be, straight from its CSS rule
 * (`max-width: min(22rem, 100vw - 1.5rem)`). A direction is judged on the
 * widest box the frame allows rather than on one measured desktop label, or a
 * long name hangs off a phone. */
export function labelWidthFor(width: number): number {
  return Math.min(22 * 16, Math.max(0, width - 24));
}

/** Where the label's left edge lands: outward from the anchor, then clamped
 * into the frame. On a phone a label is nearly as wide as the viewport, so the
 * anchor cannot be honoured — the text keeps its edge placement and the line
 * is what still points at the star. */
export function labelLeft(
  anchorX: number,
  direction: LeaderName,
  labelWidth: number,
  frameWidth: number,
): number {
  const ideal =
    direction === 'up-right' || direction === 'down-right' ? anchorX : anchorX - labelWidth;
  const limit = Math.max(EDGE_PX, frameWidth - EDGE_PX - labelWidth);
  return Math.min(Math.max(ideal, EDGE_PX), limit);
}

/** Where a direction puts the label's box, in CSS pixels. The box is not
 * centred on the anchor point, so a direction has to be judged on where the
 * *text* lands rather than on where the line ends. */
export function leaderBox(
  placement: { x: number; y: number },
  direction: LeaderName,
  offset: number,
  frame: LeaderFrame,
): { left: number; top: number; right: number; bottom: number } {
  const [ux, uy] = LEADER_VECTOR[direction];
  const anchorX = ((placement.x + 1) / 2) * frame.width + ux * offset;
  const anchorY = ((1 - placement.y) / 2) * frame.height + uy * offset;
  const [w, h] = frame.label;
  const left = labelLeft(anchorX, direction, w, frame.width);
  return { left, right: left + w, top: anchorY - h / 2, bottom: anchorY + h / 2 };
}

function overlap(low: number, high: number, otherLow: number, otherHigh: number): number {
  return Math.max(0, Math.min(high, otherHigh) - Math.max(low, otherLow));
}

/** How badly a direction breaks the two rules, in pixels of trespass: off the
 * frame, and onto the hero. Zero is a direction that breaks neither. */
export function leaderPenalty(
  placement: { x: number; y: number },
  direction: LeaderName,
  offset: number,
  frame: LeaderFrame,
): number {
  const box = leaderBox(placement, direction, offset, frame);
  // Off the frame counts double: a label with its name cut off is unreadable,
  // where one that crowds the hero is only tight.
  const off =
    2 *
    (Math.max(0, EDGE_PX - box.left) +
      Math.max(0, box.right - (frame.width - EDGE_PX)) +
      Math.max(0, EDGE_PX - box.top) +
      Math.max(0, box.bottom - (frame.height - EDGE_PX)));

  // The band is a box, so a label only trespasses on it when it overlaps on
  // both axes: a label directly above the hero is beside it, not on it.
  const bandX = (frame.keepOut[0] * frame.width) / 2;
  const bandY = (frame.keepOut[1] * frame.height) / 2;
  const across = overlap(box.left, box.right, frame.width / 2 - bandX, frame.width / 2 + bandX);
  const down = overlap(box.top, box.bottom, frame.height / 2 - bandY, frame.height / 2 + bandY);
  const hero = across > 0 && down > 0 ? across + down : 0;

  return off + hero;
}

/** Which way a callout is offset from its star: the first direction in
 * preference order that clears both the hero band and the frame, and failing
 * that the one that trespasses least. A forced placement is already clamped to
 * the edge, so this is also what turns its line inward. */
export function chooseLeader(
  placement: { x: number; y: number },
  offset: number,
  frame: LeaderFrame,
): LeaderName {
  let best: LeaderName = LEADER_ORDER[0]!;
  let bestPenalty = Number.POSITIVE_INFINITY;
  for (const direction of LEADER_ORDER) {
    const penalty = leaderPenalty(placement, direction, offset, frame);
    if (penalty <= 0.001) return direction;
    if (penalty < bestPenalty) {
      bestPenalty = penalty;
      best = direction;
    }
  }
  return best;
}

/** The leader's two ends, in CSS pixels relative to the star: from the ring's
 * rim out to the label's anchor, stopping just short of the text. */
export function leaderLine(
  direction: LeaderName,
  offset: number,
): { x1: number; y1: number; x2: number; y2: number; dx: number; dy: number } {
  const [ux, uy] = LEADER_VECTOR[direction];
  const start = RING_RADIUS_PX + 1.5;
  // Stop short of the anchor: the label's box ends exactly there, and a line
  // that reaches it runs into the last word of the detail line.
  const end = Math.max(start + 2, offset - 10);
  return {
    x1: ux * start,
    y1: uy * start,
    x2: ux * end,
    y2: uy * end,
    dx: ux * offset,
    dy: uy * offset,
  };
}
