import { afterEach, describe, expect, it } from 'vitest';

import { isFresh, impersonationEnabled, needsReauth, REAUTH_WINDOW_MINUTES } from '../reauth';
import { isLastOperator, refusesRevoke } from '../operators';

const NOW = new Date('2026-09-05T12:00:00Z');

function principal(reauthenticatedAt: Date | null) {
  return {
    personId: 1,
    displayName: 'The Operator',
    sessionId: 1,
    source: 'local' as const,
    isOperator: true,
    actingOperatorId: null,
    reauthenticatedAt,
  };
}

function minutesAgo(minutes: number): Date {
  return new Date(NOW.getTime() - minutes * 60 * 1000);
}

describe('the re-authentication window', () => {
  it('is ten minutes wide and closed by default', () => {
    expect(REAUTH_WINDOW_MINUTES).toBe(10);
    expect(isFresh(null, NOW)).toBe(false);
    expect(needsReauth(principal(null), NOW)).toBe(true);
  });

  it('holds inside the window and not a minute past it', () => {
    expect(isFresh(minutesAgo(0), NOW)).toBe(true);
    expect(isFresh(minutesAgo(9), NOW)).toBe(true);
    expect(isFresh(minutesAgo(10), NOW)).toBe(true);
    expect(isFresh(minutesAgo(10.01), NOW)).toBe(false);
    expect(needsReauth(principal(minutesAgo(11)), NOW)).toBe(true);
    expect(needsReauth(principal(minutesAgo(1)), NOW)).toBe(false);
  });

  it('refuses a stamp from the future — a clock nobody trusts is not proof', () => {
    expect(isFresh(new Date(NOW.getTime() + 60_000), NOW)).toBe(false);
  });
});

describe('impersonation as a deployment switch', () => {
  const before = process.env.RN_IMPERSONATION;
  afterEach(() => {
    if (before === undefined) delete process.env.RN_IMPERSONATION;
    else process.env.RN_IMPERSONATION = before;
  });

  it('is on unless a deployment says the word', () => {
    delete process.env.RN_IMPERSONATION;
    expect(impersonationEnabled()).toBe(true);
    process.env.RN_IMPERSONATION = 'on';
    expect(impersonationEnabled()).toBe(true);
    process.env.RN_IMPERSONATION = 'OFF';
    expect(impersonationEnabled()).toBe(false);
  });
});

describe('the last operator', () => {
  it('cannot be revoked, not even by themselves', () => {
    expect(isLastOperator([7], 7)).toBe(true);
    expect(refusesRevoke([7], 7)).toBe('last');
  });

  it('is not the rule once there are two', () => {
    expect(isLastOperator([7, 9], 7)).toBe(false);
    expect(refusesRevoke([7, 9], 7)).toBe(null);
    expect(refusesRevoke([7, 9], 9)).toBe(null);
  });

  it('is a different refusal from somebody who never held it', () => {
    expect(refusesRevoke([7, 9], 11)).toBe('no');
    expect(refusesRevoke([], 7)).toBe('no');
  });
});
