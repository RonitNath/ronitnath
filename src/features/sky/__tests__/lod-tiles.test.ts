import { describe, expect, it } from 'vitest';

import {
  angleBetween,
  coveredRadiusRad,
  frameRadiusRad,
  rankTiles,
  TILES,
  tileBounds,
  tileFor,
  tileOf,
  zenithOf,
} from '../lod-tiles';
import { TILE_COUNT } from '../lod';
import { unitVector, viewMatrix, type Vec3 } from '../sidereal';

const DEG = Math.PI / 180;

describe('the tile grid', () => {
  it('is the grid the builder cut', () => {
    expect(TILES).toHaveLength(TILE_COUNT);
    expect(tileFor(0, -90)).toBe(0);
    expect(tileFor(359.9, 89.9)).toBe(TILE_COUNT - 1);
    // 11.25 degrees of right ascension, 7.5 of declination.
    expect(tileBounds(0)).toEqual([0, 11.25, -90, -82.5]);
    expect(tileBounds(33)).toEqual([11.25, 22.5, -82.5, -75]);
  });

  it('puts a direction in the tile whose bounds contain it', () => {
    for (const id of [0, 5, 100, 400, 767]) {
      const [raLo, raHi, decLo, decHi] = tileBounds(id);
      const inside = unitVector((decLo + decHi) / 2, (raLo + raHi) / 2);
      expect(tileOf(inside)).toBe(id);
      expect(angleBetween(inside, TILES[id]!.centre)).toBeLessThan(1e-6);
    }
  });

  it('measures each tile from its own corners', () => {
    // A cell against the pole is a sliver: its half-diagonal is nothing like
    // the equatorial cell's, and treating them alike would have the frame
    // test wrong by degrees at both ends.
    const polar = TILES[0]!.radius;
    const equatorial = TILES[12 * 32]!.radius;
    expect(equatorial).toBeGreaterThan(polar);
    expect(equatorial / DEG).toBeCloseTo(6.7, 0);
  });
});

describe('the frame', () => {
  it('reaches 67 degrees at 16:9 and less on a phone', () => {
    expect(frameRadiusRad(16 / 9) / DEG).toBeCloseTo(67.6, 1);
    expect(frameRadiusRad(390 / 844) / DEG).toBeLessThan(53);
  });

  it('is centred on the zenith the view matrix defines', () => {
    const matrix = viewMatrix(Date.UTC(2026, 0, 1), 37.7749, -122.4194);
    const zenith = zenithOf(matrix);
    expect(Math.hypot(...zenith)).toBeCloseTo(1, 9);
    // Through the same matrix the zenith is (0, 0, 1): straight up.
    const forward = matrix[2]! * zenith[0]! + matrix[5]! * zenith[1]! + matrix[8]! * zenith[2]!;
    expect(forward).toBeCloseTo(1, 9);
  });
});

/** A synthetic view: the zenith on tile 400's centre, a 16:9 frame, and three
 * orbit-ahead directions well outside it. */
function synthetic(): { zenith: Vec3; radius: number; ahead: Vec3[]; aheadIds: number[] } {
  const zenith = TILES[400]!.centre;
  const radius = frameRadiusRad(16 / 9);
  // The frame reaches two thirds of the way to the horizon, so "ahead of the
  // orbit" has to be well outside it to be a separate case at all: the three
  // furthest tiles from this zenith are, by construction.
  const aheadIds = [...TILES]
    .sort((a, b) => angleBetween(b.centre, zenith) - angleBetween(a.centre, zenith))
    .slice(0, 3)
    .map((tile) => tile.id);
  return { zenith, radius, ahead: aheadIds.map((id) => TILES[id]!.centre), aheadIds };
}

describe('the priority queue', () => {
  it('takes the centre first, then the frame by distance, then ahead', () => {
    const { zenith, radius, ahead, aheadIds } = synthetic();
    const order = rankTiles(zenith, radius, ahead);

    expect(order[0]!.id).toBe(400);
    const inFrame = order.filter((need) => need.inFrame);
    const outside = order.filter((need) => !need.inFrame);

    // Every tile in the frame comes before every tile that is not.
    expect(order.slice(0, inFrame.length).every((need) => need.inFrame)).toBe(true);
    // And within the frame, nearest to the zenith first.
    for (let index = 1; index < inFrame.length; index += 1) {
      // Ties between mirror-image tiles land within a float of each other.
      expect(inFrame[index]!.distance).toBeGreaterThan(inFrame[index - 1]!.distance - 1e-9);
    }
    // The three the orbit will reach are the only ones outside it, in the
    // order the orbit reaches them.
    expect(outside.map((need) => need.id)).toEqual(aheadIds);
    expect(outside.map((need) => need.ahead)).toEqual([0, 1, 2]);
  });

  it('asks for nothing behind the observer', () => {
    const { zenith, radius } = synthetic();
    const order = rankTiles(zenith, radius);
    const worst = order.at(-1)!;
    expect(worst.distance).toBeLessThan(radius);
    expect(order.length).toBeLessThan(TILE_COUNT);
  });

  it('is stable while the sky turns a tick worth of arc', () => {
    const { zenith, radius } = synthetic();
    // A quarter of a degree — half a second at 60x — must not reshuffle the
    // head of the queue, or every tick would abandon what it just fetched.
    const nudged: Vec3 = [zenith[0] + 0.004, zenith[1], zenith[2]];
    const head = (at: Vec3, take: number): number[] =>
      rankTiles(at, radius)
        .slice(0, take)
        .map((need) => need.id);
    // The tiles the queue is actually working on do not move; further down,
    // where tiles are equidistant to a float, they may swap places.
    expect(new Set(head(nudged, 5))).toEqual(new Set(head(zenith, 5)));
    const before = new Set(head(zenith, 12));
    expect(head(nudged, 12).filter((id) => before.has(id)).length).toBeGreaterThanOrEqual(10);
  });
});

describe('the covered cone', () => {
  it('is nothing until a tile lands, and grows with them', () => {
    const zenith = TILES[400]!.centre;
    const limit = frameRadiusRad(16 / 9);
    expect(coveredRadiusRad(zenith, new Set(), limit)).toBe(0);

    const near = rankTiles(zenith, limit)
      .slice(0, 12)
      .map((need) => need.id);
    const some = coveredRadiusRad(zenith, new Set(near.slice(0, 4)), limit);
    const more = coveredRadiusRad(zenith, new Set(near), limit);
    expect(some).toBeGreaterThan(0);
    expect(more).toBeGreaterThan(some);
  });

  it('never claims more than the frame', () => {
    const zenith = TILES[400]!.centre;
    const limit = frameRadiusRad(16 / 9);
    const everything = new Set(TILES.map((tile) => tile.id));
    expect(coveredRadiusRad(zenith, everything, limit)).toBe(limit);
  });
});
