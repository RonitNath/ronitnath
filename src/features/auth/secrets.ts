/* Opaque tokens, and a hash of one.
 *
 * Passwords left this file when better-auth took the door: what a password is
 * hashed with is a fleet-wide fact and lives in `@/lib/fleet/identity`. What
 * is left is the tokens that are still ours — the `link` table's invitations
 * and claims, which are this site's own single-use URLs and have nothing to do
 * with signing in.
 *
 * Tokens are 256 bits of CSPRNG and are handed out exactly once, in a URL.
 * What the database keeps is the SHA-256 — a stolen dump is not a stolen
 * invitation — and SHA-256 is enough because the token has full entropy and so
 * cannot be guessed offline the way a password can. */

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

/* Constant-time comparison for two same-length digests. */
export function digestsEqual(a: string, b: string): boolean {
  const left = Buffer.from(a, 'utf8');
  const right = Buffer.from(b, 'utf8');
  if (left.length !== right.length) return false;
  return timingSafeEqual(left, right);
}
