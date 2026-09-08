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
export const LABEL_PX: readonly [number, number] = [328.13, 41.9];

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

/* -------------------------------------------------------------- chrome ---- */

/** A rectangle in CSS pixels, the way `getBoundingClientRect` gives one. */
export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** The same rectangle in normalised device coordinates: x right, y **up**. */
export interface NdcBox {
  x0: number;
  x1: number;
  y0: number;
  y1: number;
}

/** The header strip (`--topbar-height`), the whole width of the frame. A
 * callout drawn over it puts its detail line under the theme button, which is
 * two texts on top of each other exactly the way a callout on the hero is. */
export const TOPBAR_PX = 4.45 * 16;

/** The bottom-left block — globe, caption and the two controls — as
 * `.sky-chrome` lays it out: inset `clamp(0.5rem, 2vw, 1.25rem)` across and
 * `clamp(0.75rem, 2vh, 1.25rem)` up, around a `clamp(96px, 18vmin, 152px)`
 * globe. The caption and the controls under it are counted generously, with
 * room for a second row of controls, because this estimate is what a placement
 * runs on where nothing has been measured; the callouts measure the real
 * element when there is a DOM to measure. */
const CHROME_UNDER_GLOBE_PX = 96;

/** The caption is a coordinate pair and a city name, so the block is as wide
 * as that line rather than as wide as the globe. */
const CHROME_TEXT_PX = 300;

/** Where the hero card itself is, as against the band around it. `.home-hero`
 * fills the frame under the header and centres the card in it, with the
 * landing's asymmetric padding (`2rem` over, `5rem` under) lifting the card
 * half the difference. The band is a placement rule with headroom in it; this
 * is the box the text is actually in, and the one a label may never touch. */
const HERO_LIFT_PX = (5 - 2) * 8;

export function heroBox(width: number, height: number): Box {
  const [cardWidth, cardHeight] = heroFor(width);
  const centreY = TOPBAR_PX + (height - TOPBAR_PX) / 2 - HERO_LIFT_PX;
  return {
    left: width / 2 - cardWidth / 2,
    right: width / 2 + cardWidth / 2,
    top: centreY - cardHeight / 2,
    bottom: centreY + cardHeight / 2,
  };
}

export function topChrome(width: number): Box {
  return { left: 0, top: 0, right: width, bottom: TOPBAR_PX };
}

export function cornerChrome(width: number, height: number): Box {
  const insetX = between(0.02 * width, 8, 20);
  const insetY = between(0.02 * height, 12, 20);
  const globe = between(0.18 * Math.min(width, height), 96, 152);
  const bottom = height - insetY;
  return {
    left: insetX,
    right: insetX + Math.min(width - 2 * insetX, Math.max(globe, CHROME_TEXT_PX)),
    top: bottom - (globe + CHROME_UNDER_GLOBE_PX),
    bottom,
  };
}

/** Everything on the page that is not the sky and not the hero. */
export function chromeBoxes(width: number, height: number): Box[] {
  return [topChrome(width), cornerChrome(width, height)];
}

export function ndcBox(box: Box, width: number, height: number): NdcBox {
  return {
    x0: (box.left / width) * 2 - 1,
    x1: (box.right / width) * 2 - 1,
    y0: 1 - (box.bottom / height) * 2,
    y1: 1 - (box.top / height) * 2,
  };
}

/** How far a label can end up from the anchor it hangs off: out along the
 * leader, and then its own box, which the frame edge may push back inward but
 * never further out than that. Grow a box by this and an anchor outside the
 * result cannot put a label inside the box, whichever way the leader turns. */
function reach(width: number): readonly [number, number] {
  const offset = leaderOffset(width);
  return [labelWidthFor(width) + offset, LABEL_PX[1] / 2 + offset];
}

/** What a *placement* has to stay out of.
 *
 * The hero band that gates the ordinary path is a rule about where a label is
 * *centred*; this is the stronger statement the frame edge forced — a label
 * clamped back inside the margin no longer sits where its leader pointed, and
 * beside a card as wide as a third of a 1024 px frame there is no room left
 * for it. Grown by a whole label the box says what it means: an anchor outside
 * it has a direction that clears the card.
 *
 * The header is not here. It sits above the frame margin at every height the
 * landing is drawn at, so it can catch a label reaching up for it but never an
 * anchor, and a label is what {@link leaderPenalty} turns away. */
export function placementAvoid(width: number, height: number): NdcBox[] {
  const [outX, outY] = reach(width);
  const grown = (box: Box): NdcBox =>
    ndcBox(
      {
        left: box.left - outX,
        right: box.right + outX,
        top: box.top - outY,
        bottom: box.bottom + outY,
      },
      width,
      height,
    );
  // The corner block is grown the same way, and by the same argument. Two of
  // its sides are off the frame once grown, which is no loss: an anchor there
  // was never legal.
  return [grown(heroBox(width, height)), grown(cornerChrome(width, height))];
}

function within(x: number, y: number, boxes: readonly NdcBox[]): boolean {
  return boxes.some((box) => x >= box.x0 && x <= box.x1 && y >= box.y0 && y <= box.y1);
}

/** How far past a block a pushed placement lands: enough that the label
 * centred on it clears the block rather than resting on its edge. */
const CHROME_CLEARANCE = 0.06;

/** A clamped placement moved off whatever the page has already drawn there.
 *
 * It moves along the column it is already in — the x is what keeps a forced
 * label pointing at the right side of the sky — and takes the nearest clear
 * row above or below the blocks it is in, rather than stepping off one block
 * and onto the next, which is what happens when the blocks are cleared one at
 * a time in order. */
export function pushOutOfBoxes(x: number, y: number, boxes: readonly NdcBox[]): number {
  if (!within(x, y, boxes)) return y;
  const rows = boxes
    .flatMap((box) => [box.y1 + CHROME_CLEARANCE, box.y0 - CHROME_CLEARANCE])
    .filter((row) => Math.abs(row) <= MARGIN_Y)
    .sort((a, b) => Math.abs(a - y) - Math.abs(b - y));
  for (const row of [...rows, MARGIN_Y, -MARGIN_Y]) {
    if (!within(x, row, boxes)) return row;
  }
  // Nothing on this column is clear of everything. The top of the frame is the
  // least-bad row: the header above it is the one block a leader can turn away
  // from, because it is a label that reaches it and never an anchor.
  return MARGIN_Y;
}

/** Two callouts stacked within this much of each other in normalised device
 * coordinates are one unreadable block of text, not two labels. */
const MIN_SEPARATION: readonly [number, number] = [0.55, 0.14];

/** How far apart two anchors have to be for their labels not to overlap.
 *
 * The constant above is a desktop measurement of one label, and one label is
 * the wrong unit: a label hangs off its anchor by {@link reach} in whichever
 * direction the leader turned, so two anchors whose leaders turn *toward each
 * other* need twice that between them. Measuring one label instead is what put
 * Polaris and Deneb over each other at 403 px apart on a 1440 px frame — both
 * passed a 328 px test and both labels ran 364 px inward.
 *
 * S3 measured against the label rather than a fixed number, which is what
 * fixed the phone; S4 grew the name list from 50 stars to 333, which made two
 * of them landing near each other the common case rather than the rare one,
 * and that is when the missing factor of two showed. The test is an AND over
 * both axes, so this rejects a pair only where the boxes really would meet. */
export function separationFor(width: number): [number, number] {
  const [outX, outY] = reach(width);
  const perPx = 2 / width;
  return [
    Math.max(MIN_SEPARATION[0], 2 * outX * perPx),
    Math.max(MIN_SEPARATION[1], 2 * outY * perPx),
  ];
}

/** How many labels the frame carries at once. Three is what fits down one side
 * of a phone without the sky becoming a list. */
export const MAX_LABELS = 3;

function clamp(value: number, limit: number): number {
  return value < -limit ? -limit : value > limit ? limit : value;
}

function between(value: number, low: number, high: number): number {
  return value < low ? low : value > high ? high : value;
}

/** Project `stars` (J2000 unit vectors) and return up to `limit` placements,
 * brightest-first in catalog order. */
export function place(
  stars: readonly Vec3[],
  matrix: Mat3,
  aspect: number,
  limit: number = MAX_LABELS,
  keepOut: readonly [number, number] = KEEP_OUT,
  avoid: readonly NdcBox[] = [],
  separation: readonly [number, number] = MIN_SEPARATION,
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
    const onChrome = within(x, y, avoid);

    if (!onHero && !onChrome && Math.abs(x) <= MARGIN_X && Math.abs(y) <= MARGIN_Y) {
      const collides = inside.some(
        (other) =>
          Math.abs(other.x - x) < separation[0] &&
          Math.abs(other.y - y) < separation[1],
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
      const forcedX = clamp(x, MARGIN_X);
      bestForced = {
        star,
        position: vector,
        x: forcedX,
        y: pushOutOfBoxes(forcedX, pushOutOfBand(clamp(y, MARGIN_Y), keepOut[1]), avoid),
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
  /** The page's own chrome, measured where there is a DOM to measure it and
   * estimated by {@link chromeBoxes} where there is not. An explicit empty
   * list is a frame with no chrome on it. */
  blocks?: readonly Box[];
}

/** How much of the clamp back into the frame counts against a direction. Less
 * than a pixel of it per pixel: staying off the hero is worth more than
 * pointing exactly at the label. */
const PULL_WEIGHT = 0.25;

/** The margin a label keeps from the frame edge. Half of it read as none at
 * all: at eight pixels the last word of a detail line sat on the frame. */
export const EDGE_PX = 16;

/** The widest a label can be, straight from its CSS rule
 * (`max-width: min(22rem, 100vw - 2rem)`), which is the frame less a margin at
 * each end: a label the frame cannot hold with its margins is a label that
 * runs off it whatever this chooses. Used where a label has not been measured;
 * a mounted callout is judged on its own box. */
export function labelWidthFor(width: number): number {
  return Math.min(22 * 16, Math.max(0, width - 2 * EDGE_PX));
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
): { left: number; top: number; right: number; bottom: number; pull: number } {
  const [ux, uy] = LEADER_VECTOR[direction];
  const anchorX = ((placement.x + 1) / 2) * frame.width + ux * offset;
  const anchorY = ((1 - placement.y) / 2) * frame.height + uy * offset;
  const [w, h] = frame.label;
  const ideal = direction === 'up-right' || direction === 'down-right' ? anchorX : anchorX - w;
  const left = labelLeft(anchorX, direction, w, frame.width);
  return {
    left,
    right: left + w,
    top: anchorY - h / 2,
    bottom: anchorY + h / 2,
    pull: Math.abs(ideal - left),
  };
}

/** How much two intervals share. Zero where they only touch. */
export function overlap(low: number, high: number, otherLow: number, otherHigh: number): number {
  return Math.max(0, Math.min(high, otherHigh) - Math.max(low, otherLow));
}

/** How badly a direction breaks the rules, in pixels of trespass: off the
 * frame, onto the hero, or onto the page's own chrome. Zero is a direction
 * that breaks none of them. */
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

  // A block is a box, so a label only trespasses on one when it overlaps on
  // both axes: a label directly above the hero is beside it, not on it. What
  // is counted is the *shorter* of the two overlaps, because that is how far
  // the label would have to move to be clear of the block.
  const bandX = (frame.keepOut[0] * frame.width) / 2;
  const bandY = (frame.keepOut[1] * frame.height) / 2;
  const hero: Box = {
    left: frame.width / 2 - bandX,
    right: frame.width / 2 + bandX,
    top: frame.height / 2 - bandY,
    bottom: frame.height / 2 + bandY,
  };
  const blocks = frame.blocks ?? chromeBoxes(frame.width, frame.height);

  let onBlock = 0;
  for (const block of [hero, ...blocks]) {
    const across = overlap(box.left, box.right, block.left, block.right);
    const down = overlap(box.top, box.bottom, block.top, block.bottom);
    if (across > 0 && down > 0) onBlock += Math.min(across, down);
  }

  // What the frame edge dragged back. A clamped label keeps its margin and is
  // legal, but it no longer sits where its leader pointed — the line runs out
  // to the right of a star whose name is off to the left of it — so a
  // direction that needs no clamping is worth a little.
  return off + onBlock + PULL_WEIGHT * box.pull;
}

/** Which way a callout is offset from its star: the first direction in
 * preference order that clears the hero, the page's chrome and the frame, and
 * failing that the one that trespasses least. A forced placement is already clamped to
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
