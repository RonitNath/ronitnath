/* The identity primitives every Isoastra site shares: one hash, one way of
 * writing an address down, and one place that knows both.
 *
 * The ruling (fleet-conventions §2) is that an account copies between apps —
 * and one day between a Node stack and a Rust one — so what is stored is a PHC
 * string and nothing else. `$argon2id$v=19$m=65536,t=3,p=4$…` is read by
 * `@node-rs/argon2` here and by the `argon2` + `password-hash` crates there,
 * and neither needs to be told the parameters: they are in the string.
 *
 * That is also why `verify` ignores ARGON. The constants below are what a
 * *new* hash is made with; an old one is verified under its own parameters and
 * quietly re-made under these the next time its owner proves they know it.
 * This site's previous hashes were m=19456,t=2,p=1 — OWASP's second choice,
 * picked when this ran on a shared node — so almost every existing account
 * will be rehashed on its first sign-in and none of them will notice.
 *
 * `verifyPassword` additionally accepts better-auth's own scrypt format,
 * which is what an account created by the library before this override was
 * installed would carry. There should be none; accepting them costs a regular
 * expression and being wrong about that would lock somebody out. */

import { createHash, randomBytes } from 'node:crypto';
import { hash as argon2Hash, verify as argon2Verify } from '@node-rs/argon2';
import { verifyPassword as verifyBetterAuthScrypt } from 'better-auth/crypto';

/* `algorithm: 2` is `Algorithm.Argon2id`. The enum is an ambient const enum,
 * which `isolatedModules` cannot read across a module boundary, and argon2id
 * is the only algorithm this fleet will ask for. */
export const ARGON = {
  algorithm: 2,
  memoryCost: 65_536,
  timeCost: 3,
  parallelism: 4,
  outputLen: 32,
} as const;

/** The prefix a hash made under the current parameters starts with. */
export const ARGON_PREFIX = `$argon2id$v=19$m=${ARGON.memoryCost},t=${ARGON.timeCost},p=${ARGON.parallelism}$`;

export function hashPassword(password: string): Promise<string> {
  return argon2Hash(password, ARGON);
}

/** True when this PHC string was made under today's parameters. A false here
 *  is the signal to rehash, not to refuse. */
export function isCurrent(phc: string): boolean {
  return phc.startsWith(ARGON_PREFIX);
}

export async function verifyPassword(phc: string, password: string): Promise<boolean> {
  try {
    if (phc.startsWith('$argon2')) return await argon2Verify(phc, password);
    /* better-auth's own hasher writes `<salt hex>:<key hex>` with no prefix,
     * so "not a PHC string" is the whole test. Verifying it is the library's
     * job, not ours: importing its function is one line and cannot drift from
     * whatever scrypt parameters a future version picks. */
    if (/^[0-9a-f]{32}:[0-9a-f]{128}$/.test(phc)) {
      return await verifyBetterAuthScrypt({ hash: phc, password });
    }
    return false;
  } catch {
    return false;
  }
}

/* An address nobody registered must not answer faster than a wrong password,
 * or the sign-in form becomes a list of who has an account here. */
const DUMMY_PHC =
  '$argon2id$v=19$m=65536,t=3,p=4$cRpkjRGipBzK4sO8qmvb1A$QrSEbKmyTIRtoVx4dtNIPk7aSznuJD0hEw29DzgALnw';

export async function spendVerificationTime(password: string): Promise<void> {
  await verifyPassword(DUMMY_PHC, password);
}

/** Lower-case, trimmed, NFC. Stored once, compared once, and the only form of
 *  an address this fleet ever writes down (fleet-conventions §2). */
export function normaliseEmail(email: string): string {
  return email.normalize('NFC').trim().toLowerCase();
}

/** A random opaque identifier for a better-auth row. The library will make its
 *  own where we do not supply one; the migration script supplies one so that
 *  re-running it is idempotent. */
export function identityId(seed?: string): string {
  return seed
    ? createHash('sha256').update(`rn:identity:${seed}`).digest('base64url').slice(0, 32)
    : randomBytes(24).toString('base64url');
}
