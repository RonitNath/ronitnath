/* The four tiers. Every page and every action names the one it needs by
 * calling one of these; there is no per-route guard to forget.
 *
 * R2 makes the first three real: the session cookie resolves to a principal
 * once per request, `operator` is the relation `person → operator → platform:*`,
 * and an operator surface asked for by anyone else is a 404 — a visitor must
 * not be able to tell an internal page from a missing one. `requireOrgOperator`
 * answers off the relation table like everything else since R5. */

import { notFound, redirect } from 'next/navigation';

import { currentPrincipal, type Principal } from '@/features/auth/session';
import {
  operatedOrganization,
  type OrganizationRow,
} from '@/features/organizations/queries';

export type Tier = 'visitor' | 'member' | 'orgOperator' | 'operator';

export interface VisitorContext {
  tier: 'visitor';
  personId: null;
  principal: null;
}

export interface MemberContext {
  tier: Exclude<Tier, 'visitor'>;
  personId: number;
  principal: Principal;
}

export interface OrgOperatorContext extends MemberContext {
  tier: 'orgOperator';
  organization: OrganizationRow;
}

export type Context = VisitorContext | MemberContext;

export async function currentPersonId(): Promise<number | null> {
  return (await currentPrincipal())?.personId ?? null;
}

export async function requireVisitor(): Promise<VisitorContext> {
  return { tier: 'visitor', personId: null, principal: null };
}

/** Where to send an anonymous reader so that signing in returns them here. */
export function signInPath(next?: string): string {
  return next ? `/auth?next=${encodeURIComponent(next)}` : '/auth';
}

export async function requireMember(next?: string): Promise<MemberContext> {
  const principal = await currentPrincipal();
  if (principal === null) redirect(signInPath(next));
  return { tier: 'member', personId: principal.personId, principal };
}

/** An admin or an owner of the organization behind this handle. A visitor is
 *  sent to the door carrying where they were going; a member who is not one
 *  gets the 404 an organization that does not exist gets, because the
 *  difference is the fact a stranger would like to learn. */
export async function requireOrgOperator(handle: string): Promise<OrgOperatorContext> {
  const principal = await currentPrincipal();
  if (principal === null) redirect(signInPath(`/org/${handle}`));
  const organization = await operatedOrganization(handle, {
    personId: principal.personId,
    isOperator: principal.isOperator,
  });
  if (organization === null) notFound();
  return { tier: 'orgOperator', personId: principal.personId, principal, organization };
}

export async function requireOperator(): Promise<MemberContext> {
  const principal = await currentPrincipal();
  if (principal === null || !principal.isOperator) notFound();
  return { tier: 'operator', personId: principal.personId, principal };
}
