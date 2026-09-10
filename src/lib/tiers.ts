/* The four tiers. Every page and every action names the one it needs by
 * calling one of these; there is no per-route guard to forget.
 *
 * This is the site's own vocabulary — visitor, member, org operator, platform
 * operator — over the fleet's gates (`src/lib/fleet/gates.ts`), which is where
 * the questions are actually asked now. The two exist side by side because
 * the URL grammar has not moved yet: `requireOrgOperator` still speaks in
 * handles under `/org/...`, and `requireOrg` speaks in slugs under `/o/...`.
 * When the routes move, the pages move to the gates and this file goes.
 *
 * The session behind all of it is better-auth's: `currentPrincipal` resolves
 * the cookie to a person once per request, `operator` is still the relation
 * `person → operator → platform:*`, and an operator surface asked for by
 * anyone else is a 404 — a visitor must not be able to tell an internal page
 * from a missing one. */

import { notFound, redirect } from 'next/navigation';

import { currentPrincipal, type Principal } from '@/features/auth/principal';
import { operatedOrganization, type OrganizationRow } from '@/features/organizations/queries';
import { signInPath as doorPath, requireOperator as gateOperator } from '@/lib/fleet/gates';

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

/** Where to send an anonymous reader so that signing in returns them here.
 *  The door moved to `/auth/sign-in`; `/auth` is a permanent redirect to it. */
export function signInPath(next?: string): string {
  return doorPath(next);
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
  const { principal, personId } = await gateOperator();
  return { tier: 'operator', personId, principal };
}
