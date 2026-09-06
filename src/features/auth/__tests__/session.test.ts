import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('next/headers', () => ({ cookies: async () => null, headers: async () => null }));

const { SLIDE_AFTER_SECONDS, expiryFromNow, sessionCookieOptions, shouldSlide } = await import(
  '../session'
);

afterEach(() => {
  delete process.env.SESSION_TTL_DAYS;
  vi.unstubAllEnvs();
});

describe('the sliding window', () => {
  it('does not write for a session seen inside the last five minutes', () => {
    const now = new Date('2026-09-05T12:00:00Z');
    expect(shouldSlide(new Date('2026-09-05T11:59:00Z'), now)).toBe(false);
    expect(shouldSlide(now, now)).toBe(false);
  });

  it('writes once the session is older than five minutes at the wrist', () => {
    const now = new Date('2026-09-05T12:00:00Z');
    const edge = new Date(now.getTime() - SLIDE_AFTER_SECONDS * 1000);
    expect(shouldSlide(edge, now)).toBe(true);
    expect(shouldSlide(new Date('2026-09-05T09:00:00Z'), now)).toBe(true);
  });

  it('pushes the expiry a full TTL out from the moment it slides', () => {
    process.env.SESSION_TTL_DAYS = '30';
    const now = new Date('2026-09-05T12:00:00Z');
    expect(expiryFromNow(now).toISOString()).toBe('2026-10-05T12:00:00.000Z');
  });

  it('reads SESSION_TTL_DAYS, and falls back to 30 when it is nonsense', () => {
    const now = new Date('2026-09-05T12:00:00Z');
    process.env.SESSION_TTL_DAYS = '1';
    expect(expiryFromNow(now).toISOString()).toBe('2026-09-06T12:00:00.000Z');
    process.env.SESSION_TTL_DAYS = 'soon';
    expect(expiryFromNow(now).toISOString()).toBe('2026-10-05T12:00:00.000Z');
  });
});

describe('the session cookie', () => {
  it('is HttpOnly, SameSite=Lax, site-wide, and lives one TTL', () => {
    process.env.SESSION_TTL_DAYS = '30';
    expect(sessionCookieOptions()).toEqual({
      httpOnly: true,
      secure: false,
      sameSite: 'lax',
      path: '/',
      maxAge: 30 * 24 * 60 * 60,
    });
  });

  it('is Secure in production and not in dev, where there is no TLS to be had', () => {
    vi.stubEnv('NODE_ENV', 'production');
    expect(sessionCookieOptions().secure).toBe(true);
  });
});
