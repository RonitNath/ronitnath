import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import {
  cssPosition,
  HERO_PX,
  KEEP_OUT,
  keepOutFor,
  LABEL_PX,
  MAX_LABELS,
  parsePosition,
  place,
  positionAttribute,
} from '../annotate';
import { namedVectors, parseNamed, parseStars } from '../catalog';
import { SIM_EPOCH_MS } from '../clock';
import { FOCAL, type Vec3, viewMatrix } from '../sidereal';
import { observerAt, TRACK_PERIOD_MS } from '../track';

const vectors = (): Vec3[] =>
  namedVectors(
    parseStars(new Uint8Array(readFileSync('public/stars/bright.bin'))),
    parseNamed(readFileSync('public/stars/named.json', 'utf8')),
  );

describe('placing the callouts', () => {
  it('labels the sky at every point of the orbit, on a desktop and on a phone', () => {
    const stars = vectors();
    let forcedSeen = false;
    for (let sample = 0; sample < 400; sample += 1) {
      const simMs = SIM_EPOCH_MS + (TRACK_PERIOD_MS * sample) / 400;
      const [lat, lon] = observerAt(simMs);
      const matrix = viewMatrix(simMs, lat, lon);
      for (const [width, height] of [
        [1440, 900],
        [390, 844],
      ] as const) {
        const keepOut = keepOutFor(width, height);
        const placed = place(stars, matrix, width / height, MAX_LABELS, keepOut);
        expect(placed.length).toBeGreaterThan(0);
        expect(placed.length).toBeLessThanOrEqual(MAX_LABELS);
        for (const placement of placed) {
          expect(
            Math.abs(placement.x) >= keepOut[0] || Math.abs(placement.y) >= keepOut[1],
          ).toBe(true);
        }
        forcedSeen ||= placed.some((placement) => placement.forced);
      }
    }
    expect(forcedSeen).toBe(true);
  });

  it('clears the hero by a whole label and not by a point', () => {
    // A placement is where a label is *centred*. Asserting that the point is
    // outside the hero's own box says nothing about the 164 px of text that
    // hangs off each side of it, and that is the gap an e2e run found by
    // watching a real label sit on the hero.
    expect(KEEP_OUT[0]).toBeGreaterThanOrEqual((HERO_PX[0] + LABEL_PX[0]) / 1440);
    expect(KEEP_OUT[1]).toBeGreaterThanOrEqual((HERO_PX[1] + LABEL_PX[1]) / 900);
  });

  it('measures the band against the viewport it is drawn in', () => {
    expect(keepOutFor(1440, 900)[0]).toBeCloseTo(KEEP_OUT[0], 6);
    expect(keepOutFor(1440, 900)[1]).toBeCloseTo(KEEP_OUT[1], 6);
    // A phone's hero is nearly the whole width, so the band across it reaches
    // the whole placeable frame and only the vertical rule places a label. A
    // sliver of legal width left beside it is what put a callout on the name.
    const [phoneX, phoneY] = keepOutFor(390, 844);
    expect(phoneX).toBeGreaterThan(KEEP_OUT[0]);
    expect(phoneX).toBe(0.88);
    expect(phoneY).toBeGreaterThan(KEEP_OUT[1]);
  });

  it('never leaves a phone unlabelled, and never labels across the hero', () => {
    const stars = vectors();
    const keepOut = keepOutFor(390, 844);
    for (let sample = 0; sample < 400; sample += 1) {
      const simMs = SIM_EPOCH_MS + (TRACK_PERIOD_MS * sample) / 400;
      const [lat, lon] = observerAt(simMs);
      const placed = place(stars, viewMatrix(simMs, lat, lon), 390 / 844, MAX_LABELS, keepOut);
      expect(placed.length, `nothing named at sample ${sample}`).toBeGreaterThan(0);
      for (const placement of placed) {
        // Across, a phone's band is the whole placeable frame, so what has to
        // hold everywhere is the vertical rule.
        expect(Math.abs(placement.y), `on the hero at sample ${sample}`).toBeGreaterThanOrEqual(
          keepOut[1],
        );
      }
    }
  });

  it('never stacks two callouts on top of each other', () => {
    const stars = vectors();
    for (let sample = 0; sample < 200; sample += 1) {
      const simMs = SIM_EPOCH_MS + (TRACK_PERIOD_MS * sample) / 200;
      const [lat, lon] = observerAt(simMs);
      const placed = place(
        stars,
        viewMatrix(simMs, lat, lon),
        390 / 844,
        MAX_LABELS,
        keepOutFor(390, 844),
      );
      for (let i = 0; i < placed.length; i += 1) {
        for (let j = i + 1; j < placed.length; j += 1) {
          const [a, b] = [placed[i]!, placed[j]!];
          expect(Math.abs(a.x - b.x) >= 0.55 || Math.abs(a.y - b.y) >= 0.14).toBe(true);
        }
      }
    }
  });

  it('clamps a forced placement to the margin and admits it', () => {
    const matrix = viewMatrix(SIM_EPOCH_MS, 0, 0);
    // Undo the projection for a chosen ndc x on the view axis, then express it
    // back in J2000 through the transposed (orthonormal) basis.
    const edge = (x: number): Vec3 => {
      const view = [x / FOCAL, 0, 1];
      const norm = Math.hypot(view[0]!, 1);
      return [0, 1, 2].map((axis) =>
        [0, 1, 2].reduce((sum, row) => sum + (matrix[axis * 3 + row]! * view[row]!) / norm, 0),
      ) as unknown as Vec3;
    };
    const placed = place([edge(1.5)], matrix, 1, 3);
    expect(placed).toHaveLength(1);
    expect(placed[0]!.forced).toBe(true);
    expect(Math.abs(placed[0]!.x)).toBeLessThanOrEqual(0.88);
    // The clamp moves the label, never the star it names.
    expect(placed[0]!.position).toEqual(edge(1.5));
  });

  it('never labels a star below the horizon', () => {
    const matrix = viewMatrix(SIM_EPOCH_MS, 0, 0);
    const straightDown = [0, 1, 2].map((axis) => -matrix[axis * 3 + 2]!) as unknown as Vec3;
    expect(place([straightDown], matrix, 1.6, 3)).toHaveLength(0);
  });

  it('prefers comfortable placements over forced ones', () => {
    const [lat, lon] = observerAt(SIM_EPOCH_MS);
    const placed = place(vectors(), viewMatrix(SIM_EPOCH_MS, lat, lon), 1.6, 2);
    expect(placed.length).toBeLessThanOrEqual(2);
    expect(placed.every((placement) => !placement.forced)).toBe(true);
  });
});

describe('a callout position', () => {
  it('becomes CSS percentages with the y axis flipped', () => {
    const at = (x: number, y: number) =>
      cssPosition({ star: 0, position: [0, 0, 1], x, y, forced: false });
    expect(at(0, 0)).toEqual([50, 50]);
    expect(at(-1, 1)).toEqual([0, 0]);
    expect(at(1, -1)).toEqual([100, 100]);
  });

  it('round-trips through the attribute a click reads it back from', () => {
    const placement = {
      star: 4,
      position: [0.5, -0.5, Math.SQRT1_2] as Vec3,
      x: 0.4,
      y: -0.2,
      forced: false,
    };
    expect(parsePosition(positionAttribute(placement))).toEqual(placement.position);
  });

  it('reads nothing from a direction that is not a direction', () => {
    for (const junk of ['', '0,0,0', '1,2', 'NaN,0,1', 'a,b,c', '1,2,3,4']) {
      expect(parsePosition(junk)).toBeNull();
    }
  });
});
