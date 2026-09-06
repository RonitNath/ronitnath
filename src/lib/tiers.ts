/* The four tiers. Every page and every action names the one it needs by
 * calling one of these; there is no per-route guard to forget.
 *
 * R0 ships the visitor path for real and leaves the other three as the shape
 * the later legs fill: no session store exists yet, so member redirects to
 * /auth and the two operator tiers decline uniformly with a 404 — a signed-out
 * visitor must not be able to tell an operator surface from a missing one. */

import { notFound, redirect } from 'next/navigation';

export type Tier = 'visitor' | 'member' | 'orgOperator' | 'operator';

export interface VisitorContext {
  tier: 'visitor';
  personId: null;
}

export interface MemberContext {
  tier: Exclude<Tier, 'visitor'>;
  personId: number;
}

export type Context = VisitorContext | MemberContext;

/* R2 replaces this with the rn_session cookie lookup. */
export async function currentPersonId(): Promise<number | null> {
  return null;
}

export async function requireVisitor(): Promise<VisitorContext> {
  return { tier: 'visitor', personId: null };
}

export async function requireMember(): Promise<MemberContext> {
  const personId = await currentPersonId();
  if (personId === null) redirect('/auth');
  return { tier: 'member', personId };
}

export async function requireOrgOperator(handle: string): Promise<MemberContext> {
  void handle;
  const personId = await currentPersonId();
  if (personId === null) notFound();
  return { tier: 'orgOperator', personId };
}

export async function requireOperator(): Promise<MemberContext> {
  const personId = await currentPersonId();
  if (personId === null) notFound();
  return { tier: 'operator', personId };
}
