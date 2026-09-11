/* The four tiers. Every page and every action names the one it needs by
 * calling one of these; there is no per-route guard to forget.
 *
 * This is the site's own vocabulary — visitor, member, org operator, platform
 * operator — over the fleet's gates (`src/lib/fleet/gates.ts`), which is where
 * the questions are actually asked now. The two exist side by side because
 * this site's sharing model is its own policy module: `requireOrgOperator`
 * asks the `relation` table for admin-or-owner of one organization, which is
 * what `/o/<org>` means here, and `requireOrg` asks the same table through
 * `allows`. Both now speak the same handle under the same `/o/` prefix.
 *
 * `requireSubjectPerson` is the `/u/<user>` half of the contract's
 * `requireSubject`. The fleet gate is org-scoped — it answers for
 * `/o/<org>/u/<user>`, where an org admin may read a member — and this site's
 * user-scoped product has no org in the path at all, so the same question
 * (who is the path about, and may the asker read them?) is asked here with
 * the platform tier as the only answer above "yourself". The shapes are
 * deliberately the same so that the two collapse into one when the shared
 * `@isoastra/authority` package lands.
 *
 * The session behind all of it is better-auth's: `currentPrincipal` resolves
 * the cookie to a person once per request, `operator` is still the relation
 * `person → operator → platform:*`, and an operator surface asked for by
 * anyone else is a 404 — a visitor must not be able to tell an internal page
 * from a missing one. */

import { and, eq, isNull } from 'drizzle-orm';
import { notFound, redirect } from 'next/navigation';

import { database, schema } from '@/db/client';
import { currentPrincipal, type Principal } from '@/features/auth/principal';
import { operatedOrganization, type OrganizationRow } from '@/features/organizations/queries';
import { signInPath as doorPath, requireOperator as gateOperator } from '@/lib/fleet/gates';
import { tryDecodeId } from '@/lib/ids';
import { orgPath, userPath } from '@/lib/paths';

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
  if (principal === null) redirect(signInPath(orgPath(handle)));
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

export interface SubjectContext extends MemberContext {
  /** The `[user]` segment exactly as it appears in the path, for building the
   *  links a page draws: every one of them stays on the subject it is about. */
  user: string;
  /** Who the path is about, which is not always who is asking. */
  subjectPersonId: number;
  /** Their name, for the band that says whose page an operator is reading. */
  subjectName: string;
  /** What the page's reads are made as. The subject supplies the person —
   *  an operator reading somebody else's page sees that person's rows, not
   *  their own — and the actor supplies the platform standing, because that
   *  is what the authority table already uses to let an operator past. */
  reader: { personId: number; isOperator: boolean };
  /** True while an operator is reading somebody else's page. */
  viewingOther: boolean;
}

/** The gate on every `/u/<user>/…` page.
 *
 *  `user` is a person's public id. A visitor is sent to the door carrying
 *  where they were going. A member reading their own pages is the ordinary
 *  case. A platform operator may read another person's — that is what the
 *  tier is for, and the audit row names both of them — and anybody else gets
 *  the 404 that a person who does not exist gets, because the difference is
 *  the fact a stranger would like to learn.
 *
 *  A malformed or unknown id is the same 404: a public id that does not
 *  decode must not be distinguishable from one that decodes to somebody who
 *  is not there. */
export async function requireSubjectPerson(
  user: string,
  view = '',
): Promise<SubjectContext> {
  const principal = await currentPrincipal();
  if (principal === null) redirect(signInPath(userPath(user, view)));

  const subjectPersonId = tryDecodeId('person', user);
  if (subjectPersonId === null) notFound();
  let subjectName = principal.displayName;

  if (subjectPersonId !== principal.personId) {
    if (!principal.isOperator) notFound();
    const rows = await database()
      .select({ id: schema.person.id, displayName: schema.person.displayName })
      .from(schema.person)
      .innerJoin(schema.party, eq(schema.party.id, schema.person.id))
      .where(
        and(
          eq(schema.person.id, subjectPersonId),
          isNull(schema.person.mergedInto),
          isNull(schema.party.disabledAt),
        ),
      )
      .limit(1);
    const found = rows[0];
    if (!found) notFound();
    subjectName = found.displayName;
  }

  return {
    tier: principal.isOperator ? 'operator' : 'member',
    personId: principal.personId,
    principal,
    user,
    subjectPersonId,
    subjectName,
    reader: { personId: subjectPersonId, isOperator: principal.isOperator },
    viewingOther: subjectPersonId !== principal.personId,
  };
}
