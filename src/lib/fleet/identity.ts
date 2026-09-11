import { createHash } from 'node:crypto';
import {
  ARGON,
  ARGON_PREFIX,
  hashPassword,
  identityId as randomIdentityId,
  needsRehash,
  normalizeEmail,
  spendVerificationTime,
  verifyPassword,
} from '@isoastra/fleet-identity';

export { ARGON, ARGON_PREFIX, hashPassword, spendVerificationTime, verifyPassword };
export const isCurrent = (phc: string) => !needsRehash(phc);
export const normaliseEmail = normalizeEmail;

export function identityId(seed?: string): string {
  return seed
    ? createHash('sha256').update(`rn:identity:${seed}`).digest('base64url').slice(0, 32)
    : randomIdentityId();
}
