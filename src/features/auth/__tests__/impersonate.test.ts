/* The one thing in the impersonation path that is written out rather than
 * imported: the session cookie's signature.
 *
 * better-call's signer is internal, and the re-export that reaches it exists
 * in the type declarations but not in the runtime bundle. So the algorithm
 * lives in `impersonate.ts` and this is what pins it. The expectation is not
 * invented: it is a cookie better-auth itself issued on a real sign-in against
 * the dev server (secret `dev-secret-not-a-real-one-0123456789`, session token
 * `iJ7cYhOx4u8CMHCRMtdxZd7WntiIVkUV`), read out of the browser and compared
 * byte for byte. If a version bump changes how the library signs, this fails
 * before anybody is locked out of a session they are wearing. */

import { describe, expect, it, vi } from 'vitest';

vi.mock('next/headers', () => ({ cookies: async () => null, headers: async () => null }));

const { signCookieValue } = await import('../impersonate');

const SECRET = 'dev-secret-not-a-real-one-0123456789';
const TOKEN = 'iJ7cYhOx4u8CMHCRMtdxZd7WntiIVkUV';

describe('the session cookie signature', () => {
  it('is what better-auth put in a real cookie for the same token', () => {
    expect(signCookieValue(TOKEN, SECRET)).toBe(
      'iJ7cYhOx4u8CMHCRMtdxZd7WntiIVkUV.nf7DVX6iuFmG4oHjFgZ+lBn6G+TzfpvpTu/hXzG3qmM=',
    );
  });

  it('keeps the token in front of the dot, which is what the parser splits on', () => {
    const signed = signCookieValue('some-token', SECRET);
    expect(signed.slice(0, signed.lastIndexOf('.'))).toBe('some-token');
  });

  it('depends on the secret, or it would not be a signature', () => {
    expect(signCookieValue(TOKEN, SECRET)).not.toBe(signCookieValue(TOKEN, `${SECRET}x`));
  });
});
