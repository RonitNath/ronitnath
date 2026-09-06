/* The three primitives every credential path here shares: an opaque token, a
 * hash of one, and an argon2id password.
 *
 * Tokens are 256 bits of CSPRNG and are handed out exactly once, in a URL or a
 * cookie. What the database keeps is the SHA-256 — a stolen dump is not a
 * stolen session — and SHA-256 is enough because the token has full entropy
 * and so cannot be guessed offline the way a password can. */

import { hash as argon2Hash, verify as argon2Verify } from '@node-rs/argon2';
import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';

export const TOKEN_BYTES = 32;

export function mintToken(): string {
  return randomBytes(TOKEN_BYTES).toString('base64url');
}

export function hashToken(token: string): string {
  return createHash('sha256').update(token, 'utf8').digest('base64url');
}

export function tokenLooksWellFormed(token: string): boolean {
  return /^[A-Za-z0-9_-]{43}$/.test(token);
}

/* OWASP's second-choice argon2id parameters (19 MiB, t=2, p=1): the cheapest
 * setting the guidance still endorses, chosen because this runs inside a
 * request on a shared node. */
/* `Algorithm.Argon2id` is an ambient const enum, which `isolatedModules`
 * cannot read across a module boundary; 2 is its value and argon2id is the
 * only algorithm this file will ever ask for. */
const OPTIONS = {
  algorithm: 2,
  memoryCost: 19_456,
  timeCost: 2,
  parallelism: 1,
} as const;

export function hashPassword(password: string): Promise<string> {
  return argon2Hash(password, OPTIONS);
}

export async function verifyPassword(password: string, phc: string): Promise<boolean> {
  try {
    return await argon2Verify(phc, password, OPTIONS);
  } catch {
    return false;
  }
}

/* An address nobody registered must not answer faster than a wrong password,
 * or the sign-in form becomes a list of who has an account here. This is the
 * hash of "*" under the same parameters, verified against and discarded. */
const DUMMY_PHC =
  '$argon2id$v=19$m=19456,t=2,p=1$u0rIqcDoNKgFmhUi+EOZUA$+AOtZvS8f645+EVBH4fduzeptJbVHVXEBs1Gn4v3YHM';

export async function spendVerificationTime(password: string): Promise<void> {
  await verifyPassword(password, DUMMY_PHC);
}

/* Constant-time comparison for two same-length digests. */
export function digestsEqual(a: string, b: string): boolean {
  const left = Buffer.from(a, 'utf8');
  const right = Buffer.from(b, 'utf8');
  if (left.length !== right.length) return false;
  return timingSafeEqual(left, right);
}
