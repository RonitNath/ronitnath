import { describe, expect, it } from 'vitest';

import { KIND_GAIA, KIND_HIP, type StarCatalog } from '../catalog';
import { COLOUR_RAMP, type DeepStars } from '../lod';
import {
  colourWord,
  colourWordFromRgb,
  PICK_RADIUS_PX,
  Picker,
  teffFromLevel,
} from '../pick';
import { FOCAL, type Mat3 } from '../sidereal';

/** Straight down the view axis, so a star's sky vector and its screen position
 * are related by the projection alone. */
const IDENTITY: Mat3 = [1, 0, 0, 0, 1, 0, 0, 0, 1];
const WIDTH = 1_440;
const HEIGHT = 900;

/** The unit vector that lands on a given pixel, which is the projection in
 * `pick.ts` run backwards. Placing stars by where they should appear is the
 * only way to assert about a pick without restating the projection as the
 * expected answer. */
function towards(sx: number, sy: number): [number, number, number] {
  const aspect = WIDTH / HEIGHT;
  const vx = (((2 * sx) / WIDTH - 1) * aspect) / FOCAL;
  const vy = (1 - (2 * sy) / HEIGHT) / FOCAL;
  const norm = Math.hypot(vx, vy, 1);
  return [vx / norm, vy / norm, 1 / norm];
}

interface Star {
  at: [number, number];
  magnitude: number;
  id: bigint;
  kind?: number;
}

function catalog(stars: Star[]): StarCatalog {
  const position = new Float32Array(stars.length * 3);
  const magnitude = new Float32Array(stars.length);
  const color = new Float32Array(stars.length * 3);
  const id = new BigUint64Array(stars.length);
  const kind = new Uint8Array(stars.length);
  stars.forEach((star, index) => {
    const [x, y, z] = towards(star.at[0], star.at[1]);
    position.set([x, y, z], index * 3);
    magnitude[index] = star.magnitude;
    color.set([1, 1, 1], index * 3);
    id[index] = star.id;
    kind[index] = star.kind ?? KIND_GAIA;
  });
  return { position, magnitude, color, id, kind, count: stars.length };
}

function deep(stars: Star[]): DeepStars {
  const position = new Float32Array(stars.length * 3);
  const magnitude = new Float32Array(stars.length);
  const color = new Uint8Array(stars.length * 3);
  const sourceId = new BigUint64Array(stars.length);
  stars.forEach((star, index) => {
    position.set(towards(star.at[0], star.at[1]), index * 3);
    magnitude[index] = star.magnitude;
    color.set([255, 255, 255], index * 3);
    sourceId[index] = star.id;
  });
  return { count: stars.length, position, magnitude, color, sourceId };
}

const RAMP = COLOUR_RAMP;

function picker(bright: Star[], g9: Star[] = []): Picker {
  const created = new Picker();
  created.setCatalog(catalog(bright), RAMP);
  if (g9.length) created.setG9(deep(g9));
  created.project(IDENTITY, WIDTH, HEIGHT);
  return created;
}

describe('Picker', () => {
  it('finds the star under the pointer and names it', () => {
    const found = picker([{ at: [700, 400], magnitude: 3, id: 42n }]).pick(702, 401);
    expect(found?.key).toBe('gaia-42');
    expect(found?.source).toBe('bright');
    expect(found?.x).toBeCloseTo(700, 3);
    expect(found?.y).toBeCloseTo(400, 3);
    expect(found?.magnitude).toBe(3);
  });

  it('spells a Hipparcos record the way the route reads it', () => {
    const found = picker([
      { at: [200, 200], magnitude: 1, id: 62956n, kind: KIND_HIP },
    ]).pick(200, 200);
    expect(found?.key).toBe('hip-62956');
  });

  it('reaches fourteen pixels and no further', () => {
    const one = picker([{ at: [700, 400], magnitude: 6, id: 1n }]);
    expect(one.pick(700 + PICK_RADIUS_PX - 1, 400)).not.toBeNull();
    expect(one.pick(700 + PICK_RADIUS_PX + 1, 400)).toBeNull();
    // Nor does a *bright* star reach further than that: its size decides
    // which candidate wins, never whether there is one.
    const bright = picker([{ at: [700, 400], magnitude: -1.4, id: 1n }]);
    expect(bright.pick(700 + PICK_RADIUS_PX + 1, 400)).toBeNull();
  });

  it('weights by brightness: the big star wins from further away', () => {
    // A first-magnitude star is drawn about 8 px across; a magnitude-8 star is
    // two. Six pixels off the bright one is inside it; five off the faint one
    // is beside it.
    const found = picker([
      { at: [700, 400], magnitude: 0, id: 100n },
      { at: [711, 400], magnitude: 8, id: 200n },
    ]).pick(706, 400);
    expect(found?.key).toBe('gaia-100');
  });

  it('takes the nearer of two stars the same brightness', () => {
    const found = picker([
      { at: [700, 400], magnitude: 5, id: 100n },
      { at: [710, 400], magnitude: 5, id: 200n },
    ]).pick(707, 400);
    expect(found?.key).toBe('gaia-200');
  });

  it('picks out of g9 as well, and keys it by Gaia source id', () => {
    const found = picker([{ at: [100, 100], magnitude: 2, id: 1n }], [
      { at: [700, 400], magnitude: 8, id: 987654321n },
    ]).pick(700, 400);
    expect(found?.source).toBe('g9');
    expect(found?.key).toBe('gaia-987654321');
    expect(found?.name).toBeNull();
  });

  it('prefers the bright catalogue where a star is in both', () => {
    const found = picker(
      [{ at: [700, 400], magnitude: 8, id: 55n }],
      [{ at: [700, 400], magnitude: 8, id: 55n }],
    ).pick(700, 400);
    expect(found?.source).toBe('bright');
  });

  it('will not pick g9 records that arrived without ids', () => {
    const created = new Picker();
    created.setCatalog(catalog([{ at: [10, 10], magnitude: 2, id: 1n }]), RAMP);
    const withoutIds = deep([{ at: [700, 400], magnitude: 8, id: 9n }]);
    delete withoutIds.sourceId;
    created.setG9(withoutIds);
    created.project(IDENTITY, WIDTH, HEIGHT);
    expect(created.pick(700, 400)).toBeNull();
  });

  it('ignores what is behind the viewer', () => {
    const behind = new Picker();
    const one = catalog([{ at: [700, 400], magnitude: 2, id: 1n }]);
    // Same direction, other hemisphere.
    for (let axis = 0; axis < 3; axis += 1) one.position[axis] = -one.position[axis]!;
    behind.setCatalog(one, RAMP);
    behind.project(IDENTITY, WIDTH, HEIGHT);
    expect(behind.pick(700, 400)).toBeNull();
  });

  it('names a star that `named.json` knows', () => {
    const created = new Picker();
    created.setCatalog(catalog([{ at: [700, 400], magnitude: 1.7, id: 7n }]), RAMP);
    created.setNames(new Map([[0, 'Alioth']]));
    created.project(IDENTITY, WIDTH, HEIGHT);
    expect(created.pick(700, 400)?.name).toBe('Alioth');
    expect(created.brightHit(0)?.key).toBe('gaia-7');
    expect(created.brightHit(9)).toBeNull();
  });

  it('answers nothing before a view has been projected', () => {
    const created = new Picker();
    created.setCatalog(catalog([{ at: [700, 400], magnitude: 2, id: 1n }]), RAMP);
    expect(created.pick(700, 400)).toBeNull();
  });
});

describe('colour', () => {
  it('runs the ramp backwards to a temperature and then to a word', () => {
    expect(teffFromLevel(0)).toBeCloseTo(2_900, 0);
    expect(teffFromLevel(23)).toBeCloseTo(17_000, 0);
    expect(colourWord(3_000)).toBe('red');
    expect(colourWord(4_500)).toBe('orange');
    expect(colourWord(5_800)).toBe('yellow');
    expect(colourWord(6_800)).toBe('yellow-white');
    expect(colourWord(9_000)).toBe('white');
    expect(colourWord(20_000)).toBe('blue-white');
  });

  it('reads a catalogue colour back to the word it came from', () => {
    // The two ends of the shipped ramp, and the point where blue saturates —
    // which a single-channel inversion would put at the wrong end.
    const at = (level: number): string => colourWordFromRgb(RAMP, ...(RAMP[level] as [number, number, number]));
    expect(at(0)).toBe('red');
    expect(at(RAMP.length - 1)).toBe('blue-white');
    expect(at(15)).toBe('yellow-white');
    // Near a ramp entry rather than on it: still that entry's word.
    expect(colourWordFromRgb(RAMP, 253, 173, 90)).toBe('red');
  });
});
