import { describe, expect, it } from 'vitest';

import {
  TOKEN_BYTES,
  digestsEqual,
  hashPassword,
  hashToken,
  mintToken,
  spendVerificationTime,
  tokenLooksWellFormed,
  verifyPassword,
} from '../secrets';

describe('passwords', () => {
  it('verifies the password it hashed and nothing else', async () => {
    const phc = await hashPassword('correct horse battery');
    expect(phc.startsWith('$argon2id$')).toBe(true);
    expect(await verifyPassword('correct horse battery', phc)).toBe(true);
    expect(await verifyPassword('correct horse batteryy', phc)).toBe(false);
  });

  it('salts, so the same password hashes to two different strings', async () => {
    const [a, b] = await Promise.all([hashPassword('same'), hashPassword('same')]);
    expect(a).not.toBe(b);
  });

  it('treats a hash it cannot parse as a refusal rather than an exception', async () => {
    await expect(verifyPassword('anything', 'not-a-phc-string')).resolves.toBe(false);
  });

  it('spends verification time against the dummy hash without throwing', async () => {
    await expect(spendVerificationTime('whatever')).resolves.toBeUndefined();
  });
});

describe('tokens', () => {
  it('mints 256 bits of base64url', () => {
    const token = mintToken();
    expect(Buffer.from(token, 'base64url')).toHaveLength(TOKEN_BYTES);
    expect(tokenLooksWellFormed(token)).toBe(true);
  });

  it('does not repeat itself', () => {
    const seen = new Set(Array.from({ length: 200 }, () => mintToken()));
    expect(seen.size).toBe(200);
  });

  it('hashes a token to a stable 256-bit digest that is not the token', () => {
    const token = mintToken();
    const digest = hashToken(token);
    expect(digest).toBe(hashToken(token));
    expect(digest).not.toBe(token);
    expect(Buffer.from(digest, 'base64url')).toHaveLength(32);
  });

  it('rejects anything that is not a minted token', () => {
    for (const bad of ['', 'short', `${mintToken()}x`, `${mintToken().slice(0, 42)}+`]) {
      expect(tokenLooksWellFormed(bad)).toBe(false);
    }
  });

  it('compares digests without leaking length differences as exceptions', () => {
    const digest = hashToken('a');
    expect(digestsEqual(digest, digest)).toBe(true);
    expect(digestsEqual(digest, hashToken('b'))).toBe(false);
    expect(digestsEqual(digest, 'short')).toBe(false);
  });
});
