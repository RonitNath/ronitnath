import { describe, expect, it } from 'vitest';

import { callerKey, tokenBucket } from '../rate-limit';

/** A clock the test moves, so nothing here waits on a real one. */
function clock(): { now: () => number; advance: (ms: number) => void } {
  let at = 1_000;
  return { now: () => at, advance: (ms) => (at += ms) };
}

describe('tokenBucket', () => {
  it('lets a burst through and then refuses', () => {
    const limiter = tokenBucket(3, 1_000, clock().now);
    expect([limiter.take('a'), limiter.take('a'), limiter.take('a')]).toEqual([
      true,
      true,
      true,
    ]);
    expect(limiter.take('a')).toBe(false);
  });

  it('refills at the window rate', () => {
    const time = clock();
    const limiter = tokenBucket(4, 1_000, time.now);
    for (let i = 0; i < 4; i += 1) limiter.take('a');
    expect(limiter.take('a')).toBe(false);
    time.advance(250); // one token
    expect(limiter.take('a')).toBe(true);
    expect(limiter.take('a')).toBe(false);
    time.advance(10_000); // long past full
    for (let i = 0; i < 4; i += 1) expect(limiter.take('a')).toBe(true);
    expect(limiter.take('a')).toBe(false);
  });

  it('counts callers apart', () => {
    const limiter = tokenBucket(1, 1_000, clock().now);
    expect(limiter.take('a')).toBe(true);
    expect(limiter.take('b')).toBe(true);
    expect(limiter.take('a')).toBe(false);
  });

  it('being refused does not refill the bucket', () => {
    const time = clock();
    const limiter = tokenBucket(1, 1_000, time.now);
    limiter.take('a');
    for (let i = 0; i < 20; i += 1) expect(limiter.take('a')).toBe(false);
    time.advance(999);
    expect(limiter.take('a')).toBe(false);
    time.advance(2);
    expect(limiter.take('a')).toBe(true);
  });

  it('forgets callers that have gone quiet rather than growing without end', () => {
    const time = clock();
    const limiter = tokenBucket(2, 1_000, time.now);
    for (let i = 0; i < 1_001; i += 1) limiter.take(`caller-${i}`);
    expect(limiter.size).toBeGreaterThan(1_000);
    time.advance(2_000);
    limiter.take('someone');
    for (let i = 0; i < 1_001; i += 1) limiter.take(`later-${i}`);
    // The first thousand were swept; only the second thousand are held.
    expect(limiter.size).toBeLessThanOrEqual(1_002);
  });
});

describe('callerKey', () => {
  it('takes the client, not the hop that forwarded it', () => {
    expect(callerKey(new Headers({ 'x-forwarded-for': '203.0.113.7, 10.0.0.1' }))).toBe(
      '203.0.113.7',
    );
  });

  it('falls back to x-real-ip, then to one shared bucket', () => {
    expect(callerKey(new Headers({ 'x-real-ip': '198.51.100.4' }))).toBe('198.51.100.4');
    expect(callerKey(new Headers())).toBe('local');
    expect(callerKey(new Headers({ 'x-forwarded-for': '  ' }))).toBe('local');
  });
});
