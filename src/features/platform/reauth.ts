/* The two gates in front of the operator's sharpest commands.
 *
 * A session cookie says who is here; it does not say that the person holding
 * it is still at the keyboard. Anything that destroys or impersonates asks
 * for a password — or a fresh round trip through ZITADEL — inside the last
 * ten minutes, and `session.reauthenticated_at` is the only record of it.
 *
 * The decline is deliberately *not* uniform, and it is the one place in this
 * codebase where that is right: the caller is already inside the tier, so
 * telling them to re-authenticate reveals nothing they do not hold, and not
 * telling them leaves a control that silently does nothing. */

import type { Principal } from '@/features/auth/principal';
import { operatorPath } from '@/lib/paths';

export const REAUTH_WINDOW_MINUTES = 10;

export const REAUTH_REQUIRED =
  'Confirm it is you before doing that. The window is ten minutes.';

/** The commands that ask. Everything restorative — Enable, revoking a session
 *  — is deliberately outside it: a gate in front of undoing harm is a gate
 *  that keeps harm in place. */
export const SENSITIVE = [
  'rule-match',
  'split-merge',
  'disable-party',
  'grant-operator',
  'revoke-operator',
  'sign-in-as',
] as const;

export function isFresh(at: Date | null, now = new Date()): boolean {
  if (at === null) return false;
  const age = now.getTime() - at.getTime();
  return age >= 0 && age <= REAUTH_WINDOW_MINUTES * 60 * 1000;
}

export function needsReauth(principal: Principal, now = new Date()): boolean {
  return !isFresh(principal.reauthenticatedAt, now);
}

/** Where a refused command sends the operator back to. */
export function reauthPath(next: string): string {
  return `${operatorPath('reauth')}?next=${encodeURIComponent(next)}`;
}

/** SignInAs, switched off for a deployment that does not want it at all.
 *  Documented in .env.example and deploy/README.md. */
export function impersonationEnabled(): boolean {
  return (process.env.RN_IMPERSONATION ?? 'on').toLowerCase() !== 'off';
}
