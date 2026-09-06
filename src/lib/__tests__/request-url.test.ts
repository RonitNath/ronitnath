import { afterEach, describe, expect, it } from 'vitest';

import { publicRequestUrl, publicUrl } from '../request-url';

const original = process.env.PUBLIC_ORIGIN;
afterEach(() => {
  if (original === undefined) delete process.env.PUBLIC_ORIGIN;
  else process.env.PUBLIC_ORIGIN = original;
});

describe('publicRequestUrl', () => {
  it('replaces the bind address with the public origin and keeps path and query', () => {
    process.env.PUBLIC_ORIGIN = 'https://ronitnath.com';
    const request = new Request('https://0.0.0.0:3140/auth/oidc/callback?code=abc&state=xyz');
    expect(publicRequestUrl(request).toString()).toBe(
      'https://ronitnath.com/auth/oidc/callback?code=abc&state=xyz',
    );
  });

  it('never trusts the incoming host even when it looks public', () => {
    process.env.PUBLIC_ORIGIN = 'https://ronitnath.com';
    const request = new Request('https://evil.example/auth?next=%2Fapp');
    expect(publicRequestUrl(request).origin).toBe('https://ronitnath.com');
    expect(publicUrl('/auth?declined=1').toString()).toBe('https://ronitnath.com/auth?declined=1');
  });
});
