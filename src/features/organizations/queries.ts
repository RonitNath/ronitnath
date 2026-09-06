/* What the organization pages read. Every one of them is a read of rows the
 * caller has already been allowed to see: the tier helper asked `allows`
 * before the page rendered, and a member who may not be here got the 404 that
 * a missing organization gets. */

import { and, asc, desc, eq, inArray, isNull } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';
import { linkState, type LinkState } from '@/features/people/invitations';
import { ROLES, allows, heldRank, subjectsOf, type Actor, type Role } from '@/lib/authority';

export interface OrganizationRow {
  id: number;
  handle: string;
  name: string;
}

/** The organization behind a handle, or null. A disabled organization is
 *  null: R6's Disable takes the surface away and leaves every row and every
 *  member's own account exactly where they were, so a handle that is switched
 *  off answers the way a handle nobody ever registered answers. */
export async function organizationByHandle(handle: string): Promise<OrganizationRow | null> {
  const rows = await database()
    .select({
      id: schema.organization.id,
      handle: schema.organization.handle,
      name: schema.organization.name,
    })
    .from(schema.organization)
    .innerJoin(schema.party, eq(schema.party.id, schema.organization.id))
    .where(and(eq(schema.organization.handle, handle), isNull(schema.party.disabledAt)))
    .limit(1);
  return rows[0] ?? null;
}

/** The question `requireOrgOperator` asks: this handle, and whether the asker
 *  is an admin or an owner of it. One answer for "not yours" and "not
 *  there" — the tier turns both into the same 404. */
export async function operatedOrganization(
  handle: string,
  actor: Actor,
): Promise<OrganizationRow | null> {
  const organization = await organizationByHandle(handle);
  if (!organization) return null;
  const ok = await database().transaction((tx) =>
    allows(tx, actor, { on: 'organization', id: organization.id, need: 'admin' }),
  );
  return ok ? organization : null;
}

export interface MembershipRow extends OrganizationRow {
  role: Role;
}

/** The organizations a person is in, with the word for what they are in it.
 *  A role held through a group counts: the query is over every subject the
 *  person expands to. */
export async function listMemberships(personId: number): Promise<MembershipRow[]> {
  return database().transaction(async (tx) => {
    const subjects = await subjectsOf(tx, personId);
    const organizations = subjects.filter((row) => row.kind === 'organization');
    if (organizations.length === 0) return [];

    const rows = await tx
      .select({
        id: schema.organization.id,
        handle: schema.organization.handle,
        name: schema.organization.name,
      })
      .from(schema.organization)
      .where(inArray(schema.organization.id, organizations.map((row) => row.id)))
      .orderBy(asc(schema.organization.name));

    return Promise.all(
      rows.map(async (row) => ({
        ...row,
        role: ((await heldRank(tx, { personId, isOperator: false }, 'organization', row.id)) ??
          'member') as Role,
      })),
    );
  });
}

export interface MemberRow {
  id: number;
  displayName: string;
  held: boolean;
  role: Role;
}

/** Who is in an organization, directly. A person reached only through a group
 *  is in the group's row, not here: this table answers "who did somebody put
 *  in this organization", which is the thing an admin edits. */
export async function listMembers(organizationId: number): Promise<MemberRow[]> {
  const rows = await database()
    .select({
      id: schema.person.id,
      displayName: schema.person.displayName,
      held: schema.person.held,
      role: schema.relation.verb,
    })
    .from(schema.relation)
    .innerJoin(schema.person, eq(schema.person.id, schema.relation.subjectId))
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        inArray(schema.relation.verb, [...ROLES]),
        eq(schema.relation.resourceKind, 'organization'),
        eq(schema.relation.resourceId, organizationId),
        isNull(schema.person.mergedInto),
      ),
    )
    .orderBy(asc(schema.person.displayName));
  return rows.map((row) => ({ ...row, role: row.role as Role }));
}

export interface InvitationRow {
  linkId: number;
  personId: number;
  displayName: string;
  role: Role;
  state: LinkState;
}

/** The invitations an organization has out: a claim link pointing at a held
 *  person who already carries a role here. Claiming it merges the held person
 *  into the claimant, and the role goes with them. */
export async function listInvitations(organizationId: number): Promise<InvitationRow[]> {
  const rows = await database()
    .select({
      linkId: schema.link.id,
      personId: schema.person.id,
      displayName: schema.person.displayName,
      role: schema.relation.verb,
      openedAt: schema.link.openedAt,
      claimedAt: schema.link.claimedAt,
      revokedAt: schema.link.revokedAt,
      expiresAt: schema.link.expiresAt,
    })
    .from(schema.link)
    .innerJoin(schema.person, eq(schema.person.id, schema.link.targetId))
    .innerJoin(
      schema.relation,
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.subjectId, schema.person.id),
        inArray(schema.relation.verb, [...ROLES]),
        eq(schema.relation.resourceKind, 'organization'),
        eq(schema.relation.resourceId, organizationId),
      ),
    )
    .where(and(eq(schema.link.kind, 'claim'), eq(schema.link.targetKind, 'person')))
    .orderBy(desc(schema.link.id));
  return rows.map((row) => ({
    linkId: row.linkId,
    personId: row.personId,
    displayName: row.displayName,
    role: row.role as Role,
    state: linkState(row),
  }));
}

/** The person an invitation is for, held and not yet claimed. */
export async function heldInvitee(tx: Transaction, personId: number): Promise<boolean> {
  const rows = await tx
    .select({ held: schema.person.held })
    .from(schema.person)
    .where(and(eq(schema.person.id, personId), isNull(schema.person.mergedInto)))
    .limit(1);
  return rows[0]?.held === true;
}
