import { describe, expect, it } from 'vitest';

import {
  chooseLeader,
  keepOutFor,
  LABEL_PX,
  leaderBox,
  labelLeft,
  leaderLine,
  leaderOffset,
  leaderPenalty,
  type Placement,
  pushOutOfBand,
  RING_RADIUS_PX,
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
    const placement = at(-0.7, -0.55);
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
    expect(labelLeft(360, 'up-left', 366, 390)).toBe(8);
    expect(labelLeft(20, 'up-right', 366, 390)).toBe(16);
  });
});
