import { describe, expect, it } from 'vitest';

import { dragTo, orientation, SEGMENTS, sphere, unproject } from '../globe-math';
import { unitVector, type Vec3 } from '../sidereal';

const apply = (orient: number[], v: Vec3): Vec3 =>
  [0, 1, 2].map((row) =>
    [0, 1, 2].reduce((sum, axis) => sum + orient[axis * 3 + row]! * v[axis]!, 0),
  ) as unknown as Vec3;

describe('the globe orientation', () => {
  it('puts the observer’s own position at the point facing the viewer', () => {
    for (const [lat, lon] of [
      [0, 0],
      [37.77, -122.42],
      [-33.87, 151.21],
      [89, 12],
    ] as const) {
      const view = apply(orientation(lat, lon), unitVector(lat, lon));
      expect(Math.abs(view[0])).toBeLessThan(1e-9);
      expect(Math.abs(view[1])).toBeLessThan(1e-9);
      expect(Math.abs(view[2] - 1)).toBeLessThan(1e-9);
    }
  });

  it('puts north up and east to the right', () => {
    const orient = orientation(0, 0);
    expect(apply(orient, unitVector(90, 0))[1]).toBeGreaterThan(0.99);
    expect(apply(orient, unitVector(0, 10))[0]).toBeGreaterThan(0);
  });
});

describe('picking a point on the globe', () => {
  it('reads the centre of the disc as the point already shown', () => {
    for (const centre of [
      [0, 0],
      [37.77, -122.42],
      [-33.87, 151.21],
    ] as const) {
      const picked = unproject(0, 0, centre[0], centre[1])!;
      expect(Math.abs(picked[0] - centre[0])).toBeLessThan(1e-9);
      expect(Math.abs(picked[1] - centre[1])).toBeLessThan(1e-9);
    }
  });

  it('picks nothing off the disc rather than the nearest edge', () => {
    expect(unproject(1.01, 0, 0, 0)).toBeNull();
    expect(unproject(0.8, 0.8, 0, 0)).toBeNull();
    expect(unproject(Number.NaN, 0, 0, 0)).toBeNull();
  });

  it('picks a point further north above the centre', () => {
    expect(unproject(0, 0.5, 0, 0)![0]).toBeGreaterThan(0);
  });
});

describe('dragging the globe', () => {
  it('moves the viewpoint west and never past a pole', () => {
    expect(dragTo(0, 0, 100, 0)[1]).toBeLessThan(0);
    const lat = dragTo(80, 0, 0, 1_000)[0];
    expect(lat).toBeGreaterThanOrEqual(-90);
    expect(lat).toBeLessThanOrEqual(90);
    // Across the antimeridian rather than off the end of the number line.
    const wrapped = dragTo(0, -179, 100, 0)[1];
    expect(wrapped).toBeGreaterThan(0);
    expect(wrapped).toBeLessThan(180);
  });
});

describe('the sphere mesh', () => {
  it('is closed and indexable as u16', () => {
    const { vertices, indices } = sphere(SEGMENTS[0], SEGMENTS[1]);
    expect(vertices).toHaveLength((SEGMENTS[0] + 1) * (SEGMENTS[1] + 1) * 5);
    expect(indices).toHaveLength(SEGMENTS[0] * SEGMENTS[1] * 6);
    const vertexCount = vertices.length / 5;
    expect(indices.every((index) => index < vertexCount)).toBe(true);
    for (let i = 0; i < vertices.length; i += 5) {
      expect(
        Math.abs(Math.hypot(vertices[i]!, vertices[i + 1]!, vertices[i + 2]!) - 1),
      ).toBeLessThan(1e-6);
      expect(vertices[i + 3]!).toBeGreaterThanOrEqual(0);
      expect(vertices[i + 3]!).toBeLessThanOrEqual(1);
    }
  });
});
