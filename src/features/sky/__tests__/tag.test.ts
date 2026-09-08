import { describe, expect, it } from 'vitest';

import type { Box } from '../annotate';
import { chooseTagSide, TAG_GAP, tagBox, tagPenalty } from '../tag';

const FRAME = { width: 1440, height: 900, size: [240, 42] as [number, number] };

/** The callout's label, roughly where `annotate.ts` puts one: up and to the
 * right of its star, a label's width across. */
function calloutAt(x: number, y: number): Box {
  return { left: x + 25, right: x + 25 + 328, top: y - 46, bottom: y - 4 };
}

describe('tagBox', () => {
  it('hangs the box off the star rather than centring it on it', () => {
    const [gapX, gapY] = TAG_GAP;
    const box = tagBox(400, 300, 'right-below', FRAME);
    expect(box.left).toBe(400 + gapX);
    expect(box.top).toBe(300 + gapY);
    expect(box.right).toBe(400 + gapX + 240);
    expect(box.bottom).toBe(300 + gapY + 42);

    const flipped = tagBox(400, 300, 'left-above', FRAME);
    expect(flipped.right).toBe(400 - gapX);
    expect(flipped.bottom).toBe(300 - gapY);
  });

  it('never puts the star itself inside the box', () => {
    for (const side of ['right-below', 'left-below', 'right-above', 'left-above'] as const) {
      const box = tagBox(400, 300, side, FRAME);
      expect(box.left <= 400 && box.right >= 400 && box.top <= 300 && box.bottom >= 300).toBe(false);
    }
  });
});

describe('chooseTagSide', () => {
  it('goes right and below in an empty frame', () => {
    expect(chooseTagSide(400, 300, FRAME)).toBe('right-below');
  });

  it('flips left when the frame ends before the tag would', () => {
    const side = chooseTagSide(1420, 300, FRAME);
    expect(side).toBe('left-below');
    expect(tagPenalty(1420, 300, side, FRAME)).toBe(0);
  });

  it('flips above when the bottom of the frame is what is in the way', () => {
    expect(chooseTagSide(400, 890, FRAME)).toBe('right-above');
  });

  it('takes the far corner when both edges are close', () => {
    expect(chooseTagSide(1420, 890, FRAME)).toBe('left-above');
  });

  /* The defect this file exists for: the tag was drawn on top of a callout's
   * second line. A callout's own star draws no tag at all (`star-pick.tsx`);
   * a *neighbour* of one has to be placed clear of its box. */
  it('goes below a callout that sits above and right of the star', () => {
    const blocks = [calloutAt(400, 300)];
    const side = chooseTagSide(400, 300, { ...FRAME, blocks });
    expect(side).toBe('right-below');
    expect(tagPenalty(400, 300, side, { ...FRAME, blocks })).toBe(0);
  });

  it('flips left off a callout label lying under and right of the star', () => {
    const blocks = [{ left: 410, right: 738, top: 300, bottom: 360 }];
    const side = chooseTagSide(400, 300, { ...FRAME, blocks });
    expect(side).not.toBe('right-below');
    expect(tagPenalty(400, 300, side, { ...FRAME, blocks })).toBe(0);
  });

  it('keeps the tag off the hero card', () => {
    const hero: Box = { left: 535, right: 905, top: 380, bottom: 525 };
    const side = chooseTagSide(520, 400, { ...FRAME, blocks: [hero] });
    expect(tagPenalty(520, 400, side, { ...FRAME, blocks: [hero] })).toBe(0);
    expect(side.startsWith('left')).toBe(true);
  });

  /* The corner that hides the *least text* wins, not the one that is nearest
   * to being clear. A tag clipping the tail of a callout's detail line by ten
   * pixels of height is barely a distance and is the whole end of the line;
   * one overlapping a narrow block over its full height hides a word. */
  it('prefers hiding a sliver of one block to clipping a whole line of another', () => {
    const blocks: Box[] = [
      { left: 400, right: 1000, top: 380, bottom: 390 },
      { left: 600, right: 738, top: 410, bottom: 452 },
    ];
    const frame = { ...FRAME, blocks };
    expect(chooseTagSide(700, 400, frame)).toBe('right-below');
    // The corner the shorter-overlap rule would have taken: ten pixels from
    // clear, and the whole tail of a line under it.
    expect(tagPenalty(700, 400, 'right-below', frame)).toBeLessThan(
      tagPenalty(700, 400, 'right-above', frame),
    );
  });

  it('takes the least-bad corner when every one of them is blocked', () => {
    const everywhere: Box[] = [{ left: 0, right: 1440, top: 0, bottom: 900 }];
    const frame = { ...FRAME, blocks: everywhere };
    const side = chooseTagSide(700, 450, frame);
    const worst = Math.max(
      ...(['right-below', 'left-below', 'right-above', 'left-above'] as const).map((candidate) =>
        tagPenalty(700, 450, candidate, frame),
      ),
    );
    expect(tagPenalty(700, 450, side, frame)).toBeLessThanOrEqual(worst);
  });

  it('counts a corner off the frame as worse than one on a block', () => {
    const block: Box[] = [{ left: 418, right: 658, top: 310, bottom: 352 }];
    // A tag pushed hard against the right edge loses more than one that
    // overlaps a label by a few pixels.
    expect(tagPenalty(1439, 300, 'right-below', FRAME)).toBeGreaterThan(
      tagPenalty(400, 300, 'right-below', { ...FRAME, blocks: block }),
    );
  });
});
