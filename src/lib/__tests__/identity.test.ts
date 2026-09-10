/* The fleet's identity primitives. These moved out of features/auth/secrets.ts
 * when better-auth took the door: what a password is hashed with is a
 * fleet-wide fact now, and the property that matters most is the one that lets
 * an account copy between apps — a PHC string carries its own parameters, so a
 * hash made under yesterday's is still verifiable today. */

import { describe, expect, it } from 'vitest';

import {
  ARGON_PREFIX,
  hashPassword,
  identityId,
  isCurrent,
  normaliseEmail,
  spendVerificationTime,
  verifyPassword,
} from '@/lib/fleet/identity';

/* What this site wrote before the fleet standard existed: OWASP's second
 * choice, m=19456,t=2,p=1. `scripts/migrate-identity.ts` copies strings like
 * this one across untouched, so verifying it is the migration's whole promise.
 * The password is "correct horse battery". */
const LEGACY_PHC =
  '$argon2id$v=19$m=19456,t=2,p=1$Rz80zInoIpgQR+TXMAjyMA$UUKXmnfVB4iCI5dcPWublV0i46VJsDctekcv+eYkmjw';

describe('passwords', () => {
  it('verifies the password it hashed and nothing else', async () => {
    const phc = await hashPassword('correct horse battery');
    expect(phc.startsWith(ARGON_PREFIX)).toBe(true);
    expect(await verifyPassword(phc, 'correct horse battery')).toBe(true);
    expect(await verifyPassword(phc, 'correct horse batteryy')).toBe(false);
  });

  it('salts, so the same password hashes to two different strings', async () => {
    const [a, b] = await Promise.all([hashPassword('same'), hashPassword('same')]);
    expect(a).not.toBe(b);
  });

  it('verifies a hash made under the parameters this site used to use', async () => {
    expect(await verifyPassword(LEGACY_PHC, 'correct horse battery')).toBe(true);
    expect(await verifyPassword(LEGACY_PHC, 'wrong')).toBe(false);
  });

  it('calls an old hash old, which is the signal to re-hash rather than refuse', async () => {
    expect(isCurrent(LEGACY_PHC)).toBe(false);
    expect(isCurrent(await hashPassword('anything'))).toBe(true);
  });

  it('treats a hash it cannot parse as a refusal rather than an exception', async () => {
    await expect(verifyPassword('not-a-phc-string', 'anything')).resolves.toBe(false);
  });

  it('spends verification time against the dummy hash without throwing', async () => {
    await expect(spendVerificationTime('whatever')).resolves.toBeUndefined();
  });
});

describe('addresses and ids', () => {
  it('folds case, trims, and normalises to NFC', () => {
    expect(normaliseEmail('  Ronit@Isoastra.com ')).toBe('ronit@isoastra.com');
    expect(normaliseEmail('é@x.com')).toBe(normaliseEmail('é@x.com'));
  });

  it('derives the same id from the same seed, which is what makes the migration idempotent', () => {
    expect(identityId('ronit@isoastra.com')).toBe(identityId('ronit@isoastra.com'));
    expect(identityId('a@x.com')).not.toBe(identityId('b@x.com'));
    expect(identityId()).not.toBe(identityId());
  });
});
