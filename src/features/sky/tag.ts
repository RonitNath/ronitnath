/** Where the hover tag goes, and what is already in the way.
 *
 * The tag says a star's name, magnitude and colour beside the star. A callout
 * says the same star's name in a bigger hand a few pixels away, so the two
 * texts were printed over each other the moment a pointer found a star the
 * page had already labelled (`star-pick.tsx` answers that case by drawing no
 * tag at all). What is left here is the ordinary case: a tag beside some other
 * star, which must still not land on a callout's label, on the hero card, or
 * on any of the page's own chrome.
 *
 * The rule is the leader's rule from `annotate.ts`, one dimension smaller: the
 * four corners the tag can hang off its star, in preference order, and the
 * first that trespasses on nothing. Below-and-right is the default because the
 * callouts prefer up-and-out, so the tag's first choice is the one place a
 * callout beside the same star is least likely to be.
 *
 * The boxes it is judged against are read off the DOM without touching layout:
 * a mounted callout has already measured its own label into
 * `dataset.labelWidth/Height`, and the frame loop wrote where the group is.
 * Asking for `getBoundingClientRect` once a frame, per callout, is the one
 * thing the annotation loop is built not to do.
 */

import { type Box, EDGE_PX, overlap } from './annotate';

/** Which corner of its star the tag hangs off. */
export type TagSide = 'right-below' | 'left-below' | 'right-above' | 'left-above';

/** Preference order: out to the right and down, then across, then up. */
export const TAG_ORDER: readonly TagSide[] = [
  'right-below',
  'left-below',
  'right-above',
  'left-above',
];

/** How far the tag sits from the star it names, in CSS pixels. Across is the
 * wider gap: the star's own glare reaches further sideways in a tag's line of
 * sight than the half-line of text above or below it. */
export const TAG_GAP: readonly [number, number] = [18, 10];

/** The tag before it has been measured — the widest the CSS lets it be
 * (`max-width: 15rem`) and two lines of `--text-1`. */
export const TAG_PX: readonly [number, number] = [240, 42];

export interface TagFrame {
  /** Viewport, CSS pixels. */
  width: number;
  height: number;
  /** The tag's own box, CSS pixels. */
  size: readonly [number, number];
  /** The gap between star and tag. Defaults to {@link TAG_GAP}. */
  gap?: readonly [number, number];
  /** What the tag may not sit on: callout labels, the hero card, the page's
   * chrome, the detail panel. An empty list is a clear frame. */
  blocks?: readonly Box[];
}

/** Where a side puts the tag, in CSS pixels. The box hangs off the star's
 * corner rather than being centred on it, so the star itself is never under
 * the text. */
export function tagBox(x: number, y: number, side: TagSide, frame: TagFrame): Box {
  const [w, h] = frame.size;
  const [gapX, gapY] = frame.gap ?? TAG_GAP;
  const right = side === 'right-below' || side === 'right-above';
  const below = side === 'right-below' || side === 'left-below';
  const left = right ? x + gapX : x - gapX - w;
  const top = below ? y + gapY : y - gapY - h;
  return { left, right: left + w, top, bottom: top + h };
}

/** How badly a side breaks the rules, in pixels of trespass: off the frame, or
 * onto something the page has already drawn. Zero breaks none of them.
 *
 * Off the frame counts double, for the reason `leaderPenalty` gives: a name
 * cut off by the edge is unreadable, where one that crowds a callout is only
 * tight.
 *
 * A block costs the area it loses, over the tag's own height — which is the
 * *width of text the tag hides*, and reduces to exactly that where the tag
 * lies across a line of it. `annotate.ts` counts the shorter of the two
 * overlaps instead, because a callout it turns away can be moved and what it
 * wants is the distance; a tag has four corners and no other move, so what it
 * wants is which corner hides the least. Clipping the last inch of a callout's
 * detail line by two pixels of height is barely a distance and is all of the
 * text — that is the case that chose the wrong corner. */
export function tagPenalty(x: number, y: number, side: TagSide, frame: TagFrame): number {
  const box = tagBox(x, y, side, frame);
  const off =
    2 *
    (Math.max(0, EDGE_PX - box.left) +
      Math.max(0, box.right - (frame.width - EDGE_PX)) +
      Math.max(0, EDGE_PX - box.top) +
      Math.max(0, box.bottom - (frame.height - EDGE_PX)));

  const height = Math.max(1, frame.size[1]);
  let onBlock = 0;
  for (const block of frame.blocks ?? []) {
    const across = overlap(box.left, box.right, block.left, block.right);
    const down = overlap(box.top, box.bottom, block.top, block.bottom);
    if (across > 0 && down > 0) onBlock += (across * down) / height;
  }
  return off + onBlock;
}

/** Which corner the tag hangs off: the first in preference order that clears
 * everything, and failing that the one that trespasses least. */
export function chooseTagSide(x: number, y: number, frame: TagFrame): TagSide {
  let best: TagSide = TAG_ORDER[0]!;
  let bestPenalty = Number.POSITIVE_INFINITY;
  for (const side of TAG_ORDER) {
    const penalty = tagPenalty(x, y, side, frame);
    if (penalty <= 0.001) return side;
    if (penalty < bestPenalty) {
      bestPenalty = penalty;
      best = side;
    }
  }
  return best;
}

/* ----------------------------------------------------------------- DOM ---- */

/** A pixel length written by `callouts.tsx` as a custom property. */
function px(value: string): number {
  const number = Number.parseFloat(value);
  return Number.isFinite(number) ? number : 0;
}

/** Where the frame loop translated a callout group to, off the transform it
 * wrote. Reading the string back is not a layout read; asking the element
 * where it is would be. */
function groupOrigin(node: HTMLElement): [number, number] | null {
  const match = /translate3d\(\s*(-?[\d.]+)px\s*,\s*(-?[\d.]+)px/.exec(node.style.transform);
  if (!match) return null;
  return [Number(match[1]), Number(match[2])];
}

/** Every callout label's box, as the page has it this frame. A callout that
 * has not been placed or measured yet contributes nothing: it is not on screen
 * to be sat on. */
export function calloutBoxes(): Box[] {
  const boxes: Box[] = [];
  for (const node of document.querySelectorAll<HTMLElement>('.star-callout')) {
    const at = groupOrigin(node);
    if (!at) continue;
    const width = Number(node.dataset.labelWidth);
    const height = Number(node.dataset.labelHeight);
    if (!(width > 0) || !(height > 0)) continue;
    // The label is `translate(--callout-x, calc(--lead-y - 50%))` off the
    // group's origin, which is the star (`sky-chrome.css`).
    const left = at[0] + px(node.style.getPropertyValue('--callout-x'));
    const top = at[1] + px(node.style.getPropertyValue('--lead-y')) - height / 2;
    boxes.push({ left, right: left + width, top, bottom: top + height });
  }
  return boxes;
}

/** The page's own boxes, measured. The hero card is here and not in
 * `measureChrome` because a callout is kept off the hero by the placement band
 * that surrounds it, where a tag goes wherever its star is and has only the
 * card itself to avoid. */
export function pageBoxes(): Box[] {
  const boxes: Box[] = [];
  for (const selector of ['.topbar', '.sky-chrome', '.home-card', '.star-detail']) {
    const node = document.querySelector(selector);
    if (!node) continue;
    const box = node.getBoundingClientRect();
    if (box.width > 0 && box.height > 0)
      boxes.push({ left: box.left, top: box.top, right: box.right, bottom: box.bottom });
  }
  return boxes;
}
