import { describe, expect, it } from 'vitest';

import { fromWallClock, offsetMinutes, readableWindow, toWallClock } from '../time';

const LA = 'America/Los_Angeles';

describe('wall clocks', () => {
  it('reads a host’s clock as the instant it names', () => {
    /* 2026-08-30 14:00 in Los Angeles is PDT, seven hours behind UTC. */
    expect(fromWallClock('2026-08-30T14:00', LA)?.toISOString()).toBe('2026-08-30T21:00:00.000Z');
  });

  it('round-trips through the input value', () => {
    const at = new Date('2026-12-25T03:30:00Z');
    expect(fromWallClock(toWallClock(at, LA), LA)?.toISOString()).toBe(at.toISOString());
  });

  it('knows the zone changed under it', () => {
    expect(offsetMinutes(new Date('2026-08-30T21:00:00Z'), LA)).toBe(-420);
    expect(offsetMinutes(new Date('2026-12-25T21:00:00Z'), LA)).toBe(-480);
  });

  it('refuses what is not a wall clock', () => {
    expect(fromWallClock('sometime sunday', LA)).toBeNull();
  });
});

describe('readableWindow', () => {
  it('says the day and the hours', () => {
    const window = readableWindow(
      new Date('2026-08-30T21:00:00Z'),
      new Date('2026-08-31T02:00:00Z'),
      LA,
    );
    expect(window).toBe('Sunday, August 30, 2:00 PM – 7:00 PM');
  });

  it('says one time when there is no end', () => {
    expect(readableWindow(new Date('2026-08-30T21:00:00Z'), null, LA)).toBe(
      'Sunday, August 30, 2:00 PM',
    );
  });
});
