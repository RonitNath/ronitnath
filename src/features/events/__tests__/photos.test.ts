import { describe, expect, it } from 'vitest';

import { GRACE_MS, mayAddPhoto, photosOpen } from '../photos';

const HOUR = 60 * 60 * 1000;
const at = (ms: number) => new Date(ms);

const evening = (start: number, end: number | null = null) => ({
  publishedAt: at(0),
  startsAt: at(start),
  endsAt: end === null ? null : at(end),
});

describe('photosOpen', () => {
  it('is shut on a draft, however long ago it was meant to start', () => {
    expect(photosOpen({ ...evening(0), publishedAt: null }, 10 * HOUR)).toBe(false);
  });

  it('is shut until the evening starts', () => {
    const one = evening(10 * HOUR, 14 * HOUR);
    expect(photosOpen(one, 9 * HOUR)).toBe(false);
    expect(photosOpen(one, 10 * HOUR)).toBe(true);
    expect(photosOpen(one, 12 * HOUR)).toBe(true);
  });

  it('stays open for a fortnight past the end, and not a moment longer', () => {
    const one = evening(10 * HOUR, 14 * HOUR);
    expect(photosOpen(one, 14 * HOUR + GRACE_MS)).toBe(true);
    expect(photosOpen(one, 14 * HOUR + GRACE_MS + 1)).toBe(false);
  });

  it('counts the fortnight from the start when there is no end', () => {
    const one = evening(10 * HOUR);
    expect(photosOpen(one, 10 * HOUR + GRACE_MS)).toBe(true);
    expect(photosOpen(one, 10 * HOUR + GRACE_MS + 1)).toBe(false);
  });
});

describe('mayAddPhoto', () => {
  const one = evening(10 * HOUR, 14 * HOUR);

  it('takes a yes and nothing else', () => {
    expect(mayAddPhoto(one, { response: 'yes' }, 12 * HOUR)).toBe(true);
    expect(mayAddPhoto(one, { response: 'maybe' }, 12 * HOUR)).toBe(false);
    expect(mayAddPhoto(one, { response: 'no' }, 12 * HOUR)).toBe(false);
    expect(mayAddPhoto(one, null, 12 * HOUR)).toBe(false);
  });

  it('is shut to a yes outside the window', () => {
    expect(mayAddPhoto(one, { response: 'yes' }, 9 * HOUR)).toBe(false);
  });
});
