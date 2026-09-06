import { describe, expect, it } from 'vitest';

import {
  chooseLeader,
  chromeBoxes,
  cornerChrome,
  EDGE_PX,
  keepOutFor,
  labelWidthFor,
  LABEL_PX,
  leaderBox,
  labelLeft,
  leaderLine,
  leaderOffset,
  leaderPenalty,
  ndcBox,
  type Placement,
  pushOutOfBand,
  pushOutOfBoxes,
  RING_RADIUS_PX,
  topChrome,
  TOPBAR_PX,
} from '../annotate';
import { calloutFrame, calloutSetKey } from '../callouts';

const DESKTOP = { width: 1440, height: 900 };
const PHONE = { width: 390, height: 844 };

function frameFor(size: { width: number; height: number }) {
  return {
    width: size.width,
    height: size.height,
    label: LABEL_PX,
    keepOut: keepOutFor(size.width, size.height),
  };
}

function at(x: number, y: number, forced = false): Placement {
  return { star: 0, position: [1, 0, 0], x, y, forced };
}

describe('which way a callout is offset from its star', () => {
  it('prefers up-and-right when nothing is in the way', () => {
    // Upper left: clear of the hero, of the header and of the corner block.
    const placement = at(-0.7, 0.55);
    const frame = frameFor(DESKTOP);
    expect(leaderPenalty(placement, 'up-right', LEADER, frame)).toBe(0);
    expect(chooseLeader(placement, LEADER, frame)).toBe('up-right');
  });

  it('turns down when the label would leave the top of the frame', () => {
    // Hard against the top margin: anything up-going puts the text off-frame.
    const placement = at(-0.7, 0.995);
    const frame = frameFor(DESKTOP);
    expect(leaderPenalty(placement, 'up-right', LEADER, frame)).toBeGreaterThan(0);
    expect(chooseLeader(placement, LEADER, frame)).toBe('down-right');
  });

  it('turns away from the hero rather than laying the label across it', () => {
    // Just above the band, dead centre: going down puts the text on the name,
    // going up clears it.
    const frame = frameFor(DESKTOP);
    const placement = at(0, frame.keepOut[1] + 0.04);
    expect(leaderPenalty(placement, 'down-right', LEADER, frame)).toBeGreaterThan(0);
    expect(leaderPenalty(placement, 'down-left', LEADER, frame)).toBeGreaterThan(0);
    const chosen = chooseLeader(placement, LEADER, frame);
    expect(chosen).toBe('up-right');
    expect(leaderPenalty(placement, chosen, LEADER, frame)).toBe(0);
    const box = leaderBox(placement, chosen, LEADER, frame);
    expect(box.bottom).toBeLessThan(frame.height / 2 - (frame.keepOut[1] * frame.height) / 2);
  });

  it('runs the line from the ring to the label and nowhere else', () => {
    const line = leaderLine('up-right', LEADER);
    expect(Math.hypot(line.x1, line.y1)).toBeCloseTo(RING_RADIUS_PX + 1.5, 6);
    expect(Math.hypot(line.dx, line.dy)).toBeCloseTo(LEADER, 6);
    expect(Math.hypot(line.x2, line.y2)).toBeLessThan(Math.hypot(line.dx, line.dy));
    expect(line.y1).toBeLessThan(0);
    expect(line.x1).toBeGreaterThan(0);
  });

  it('offsets less on a phone than on a desktop', () => {
    expect(leaderOffset(PHONE.width)).toBeLessThan(leaderOffset(DESKTOP.width));
  });
});

const LEADER = leaderOffset(DESKTOP.width);

describe('the forced placement a phone falls back to', () => {
  it('is pushed clear of the hero band rather than laid across the name', () => {
    const [, keepOutY] = keepOutFor(PHONE.width, PHONE.height);
    expect(Math.abs(pushOutOfBand(0.02, keepOutY))).toBeGreaterThan(keepOutY);
    expect(Math.abs(pushOutOfBand(-0.02, keepOutY))).toBeGreaterThan(keepOutY);
    // A placement already clear of the band is left where it is.
    expect(pushOutOfBand(0.8, keepOutY)).toBe(0.8);
    // And it moves to the side of the frame the star is already on.
    expect(pushOutOfBand(-0.02, keepOutY)).toBeLessThan(0);
  });
});

describe('what is a render and what is a frame', () => {
  it('names a set by its stars, so only a change of set is a render', () => {
    expect(calloutSetKey([at(0.1, 0.2), at(0.3, 0.4)])).toBe(
      calloutSetKey([at(-0.9, -0.8), at(0.5, 0.5)]),
    );
    const moved: Placement[] = [{ ...at(0.1, 0.2), star: 7 }];
    expect(calloutSetKey(moved)).not.toBe(calloutSetKey([at(0.1, 0.2)]));
    expect(calloutSetKey([at(0.1, 0.2, true)])).not.toBe(calloutSetKey([at(0.1, 0.2)]));
  });

  it('turns a placement into the pixels one frame writes', () => {
    const frame = calloutFrame(at(0, 0.5), DESKTOP.width, DESKTOP.height);
    expect(frame.x).toBeCloseTo(720, 6);
    expect(frame.y).toBeCloseTo(225, 6);
    // The label runs outward from its anchor, never back over its own star.
    expect(labelLeft(760, 'up-right', 300, 1440)).toBe(760);
    expect(labelLeft(760, 'up-left', 300, 1440)).toBe(460);
    // And on a frame narrower than the label it keeps its edge placement.
    expect(labelLeft(360, 'up-left', 400, 390)).toBe(EDGE_PX);
    expect(labelLeft(20, 'up-right', 400, 390)).toBe(EDGE_PX);
    // A label the frame can hold keeps a full margin at the right edge too.
    expect(labelLeft(380, 'up-right', labelWidthFor(390), 390)).toBe(
      390 - EDGE_PX - labelWidthFor(390),
    );
  });
});

describe("the page's own chrome is a keep-out too", () => {
  it('turns a label down rather than laying it over the header', () => {
    // Hard against the top margin on a phone: up-going puts the text over the
    // "Sign in" row, which is where the owner found a callout drawn.
    const frame = frameFor(PHONE);
    const offset = leaderOffset(PHONE.width);
    const placement = at(-0.2, 0.82);
    expect(leaderPenalty(placement, 'up-right', offset, frame)).toBeGreaterThan(0);
    expect(leaderPenalty(placement, 'up-left', offset, frame)).toBeGreaterThan(0);
    const chosen = chooseLeader(placement, offset, frame);
    expect(chosen.startsWith('down-')).toBe(true);
    expect(leaderBox(placement, chosen, offset, frame).top).toBeGreaterThanOrEqual(TOPBAR_PX);
  });

  it('turns a label away from the globe and the caption', () => {
    const frame = frameFor(DESKTOP);
    const offset = leaderOffset(DESKTOP.width);
    const corner = cornerChrome(DESKTOP.width, DESKTOP.height);
    // Just above the corner block, over it across: going down lays the text on
    // the caption, going up clears it.
    const placement = at(
      ((corner.left + 40) / DESKTOP.width) * 2 - 1,
      1 - ((corner.top - 6) / DESKTOP.height) * 2,
    );
    expect(leaderPenalty(placement, 'down-right', offset, frame)).toBeGreaterThan(0);
    expect(chooseLeader(placement, offset, frame).startsWith('up-')).toBe(true);
  });

  it('measures the chrome where a page can be measured and estimates it where it cannot', () => {
    // The estimate is what a placement runs on, so it may never be smaller
    // than the block it stands in for. These are the boxes the shipped page
    // reports at each viewport.
    const measured = [
      [1440, 900, { left: 20, top: 660.2, right: 303, bottom: 882 }],
      [1024, 768, { left: 20, top: 544.6, right: 290, bottom: 752.6 }],
      [390, 844, { left: 8, top: 661.3, right: 278, bottom: 827.1 }],
    ] as const;
    for (const [width, height, box] of measured) {
      const corner = cornerChrome(width, height);
      expect(corner.left).toBeLessThanOrEqual(box.left);
      expect(corner.right).toBeGreaterThanOrEqual(box.right);
      expect(corner.top).toBeLessThanOrEqual(box.top);
      expect(corner.bottom).toBeGreaterThanOrEqual(box.bottom);
    }
    expect(chromeBoxes(1440, 900)).toHaveLength(2);
    expect(topChrome(1440)).toEqual({ left: 0, top: 0, right: 1440, bottom: TOPBAR_PX });
  });

  it('moves a clamped placement off the corner block', () => {
    const corner = ndcBox(cornerChrome(PHONE.width, PHONE.height), PHONE.width, PHONE.height);
    const middle = (corner.x0 + corner.x1) / 2;
    const on = (corner.y0 + corner.y1) / 2;
    expect(pushOutOfBoxes(middle, on, [corner])).toBeGreaterThan(corner.y1);
    // A placement beside the block, or above it, is left where it is.
    expect(pushOutOfBoxes(0.9, on, [corner])).toBe(on);
    expect(pushOutOfBoxes(middle, 0.5, [corner])).toBe(0.5);
  });
});
