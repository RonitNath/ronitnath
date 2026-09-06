import { describe, expect, it } from 'vitest';

import { hashToken, mintToken, tokenLooksWellFormed } from '@/features/auth/secrets';
import { claimUrl, linkState } from '../invitations';

const NEVER: Parameters<typeof linkState>[0] = {
  openedAt: null,
  claimedAt: null,
  revokedAt: null,
  expiresAt: new Date(Date.now() + 86_400_000),
};

describe('claim link tokens', () => {
  it('are 256 bits, base64url, and the same shape as a session token', () => {
    const token = mintToken();
    expect(tokenLooksWellFormed(token)).toBe(true);
    expect(Buffer.from(token, 'base64url')).toHaveLength(32);
  });

  it('are stored as a digest, not as themselves', () => {
    const token = mintToken();
    const digest = hashToken(token);
    expect(digest).not.toBe(token);
    /* Deterministic, so the URL can be looked up, and not reversible, so a
     * stolen dump is not a stolen invitation. */
    expect(hashToken(token)).toBe(digest);
    expect(Buffer.from(digest, 'base64url')).toHaveLength(32);
  });

  it('appear in a URL exactly once, under /links', () => {
    const token = mintToken();
    expect(claimUrl(token)).toBe(`http://localhost:3000/links/${token}`);
  });
});

describe('linkState', () => {
  it('says a link nobody has looked at is not the same as one nobody has claimed', () => {
    expect(linkState(NEVER)).toBe('live');
    expect(linkState({ ...NEVER, openedAt: new Date() })).toBe('opened');
  });

  it('lets a claim outrank everything else that could have happened', () => {
    const at = new Date();
    expect(linkState({ ...NEVER, openedAt: at, claimedAt: at, revokedAt: at })).toBe('claimed');
  });

  it('reads a revocation before an expiry, and an expiry before a live link', () => {
    const past = new Date(Date.now() - 1000);
    expect(linkState({ ...NEVER, revokedAt: past })).toBe('revoked');
    expect(linkState({ ...NEVER, expiresAt: past })).toBe('expired');
  });
});
