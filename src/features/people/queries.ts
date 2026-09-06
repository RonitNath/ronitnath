/* Reads for the member's own surfaces. Separate from actions.ts because
 * everything exported from a `'use server'` module is a callable endpoint,
 * and a query does not need to be one. */

import { and, desc, eq, inArray, isNull, ne, or, sql } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import { CONTACT } from './authority';
import { linkState, type LinkState } from './invitations';

export interface Contact {
  id: number;
  displayName: string;
  handle: string | null;
  /* False once they have claimed their entry: the same row in the member's
   * list, now a person who can sign in. */
  held: boolean;
  createdAt: Date;
  link: {
    id: number;
    state: LinkState;
    openedAt: Date | null;
    claimedAt: Date | null;
    expiresAt: Date | null;
  } | null;
}

/** The people this member holds, with the state of the newest invitation for
 *  each. A claim does not remove anyone from this list: the merge repointed
 *  the `contact` edge at the person who signed up, so the row stays and stops
 *  being held. That is the whole of what the member wanted to know. */
export async function listContacts(personId: number): Promise<Contact[]> {
  const db = database();
  const people = await db
    .select({
      id: schema.person.id,
      displayName: schema.person.displayName,
      held: schema.person.held,
      createdAt: schema.person.createdAt,
    })
    .from(schema.relation)
    .innerJoin(schema.person, eq(schema.person.id, schema.relation.resourceId))
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.subjectId, personId),
        eq(schema.relation.verb, CONTACT),
        eq(schema.relation.resourceKind, 'person'),
        isNull(schema.person.mergedInto),
      ),
    )
    .orderBy(desc(schema.person.createdAt));
  if (people.length === 0) return [];

  const ids = people.map((row) => row.id);
  const handles = await db
    .select({ personId: schema.identity.personId, subject: schema.identity.subject })
    .from(schema.identity)
    .where(and(eq(schema.identity.source, 'handle'), inArray(schema.identity.personId, ids)))
    .orderBy(schema.identity.id);
  /* An invitation points at the person it was minted for, and that person may
   * since have been folded into the one who claimed it; the link belongs to
   * whoever the target resolves to now. */
  const links = await db
    .select({
      id: schema.link.id,
      targetId: sql<number>`coalesce(${schema.person.mergedInto}, ${schema.person.id})`,
      openedAt: schema.link.openedAt,
      claimedAt: schema.link.claimedAt,
      revokedAt: schema.link.revokedAt,
      expiresAt: schema.link.expiresAt,
    })
    .from(schema.link)
    .innerJoin(schema.person, eq(schema.person.id, schema.link.targetId))
    .where(
      and(
        eq(schema.link.kind, 'claim'),
        eq(schema.link.targetKind, 'person'),
        eq(schema.link.createdBy, personId),
        inArray(sql`coalesce(${schema.person.mergedInto}, ${schema.person.id})`, ids),
      ),
    )
    .orderBy(desc(schema.link.id));

  const handleOf = new Map<number, string>();
  for (const row of handles) if (!handleOf.has(row.personId)) handleOf.set(row.personId, row.subject);
  const linkOf = new Map<number, (typeof links)[number]>();
  for (const row of links) if (!linkOf.has(row.targetId)) linkOf.set(row.targetId, row);

  return people.map((row) => {
    const link = linkOf.get(row.id);
    return {
      ...row,
      handle: handleOf.get(row.id) ?? null,
      link: link
        ? {
            id: link.id,
            state: linkState(link),
            openedAt: link.openedAt,
            claimedAt: link.claimedAt,
            expiresAt: link.expiresAt,
          }
        : null,
    };
  });
}

export interface Proposal {
  id: number;
  /* The address both sides spell. */
  subject: string;
  /* Who holds the contact, in their own words. */
  holderName: string | null;
  heldName: string;
  heldPersonId: number;
}

/** Matches waiting on this member's answer: somebody's contact card looks
 *  like an address this member has confirmed. The operator's half of the
 *  queue — ruling on matches between two people who are not the asker — is
 *  R6; this read deliberately only ever returns the member's own. */
export async function listProposals(personId: number): Promise<Proposal[]> {
  const db = database();
  const mine = db
    .select({ id: schema.identity.id })
    .from(schema.identity)
    .where(eq(schema.identity.personId, personId));

  const a = schema.identity;
  const rows = await db
    .select({
      id: schema.match.id,
      identityA: schema.match.identityA,
      identityB: schema.match.identityB,
      subject: a.subject,
    })
    .from(schema.match)
    .innerJoin(a, eq(a.id, schema.match.identityA))
    .where(
      and(
        eq(schema.match.status, 'proposed'),
        or(inArray(schema.match.identityA, mine), inArray(schema.match.identityB, mine)),
      ),
    )
    .orderBy(desc(schema.match.score), desc(schema.match.id));
  if (rows.length === 0) return [];

  /* The other half of each pair is the held person; who holds them is the
   * `contact` edge pointing at them. */
  const others = await db
    .select({
      identityId: schema.identity.id,
      personId: schema.identity.personId,
      displayName: schema.person.displayName,
      held: schema.person.held,
      mergedInto: schema.person.mergedInto,
    })
    .from(schema.identity)
    .innerJoin(schema.person, eq(schema.person.id, schema.identity.personId))
    .where(
      inArray(
        schema.identity.id,
        rows.flatMap((row) => [row.identityA, row.identityB]),
      ),
    );
  const byIdentity = new Map(others.map((row) => [row.identityId, row]));

  const holders = await db
    .select({
      heldId: schema.relation.resourceId,
      holderName: schema.person.displayName,
    })
    .from(schema.relation)
    .innerJoin(schema.person, eq(schema.person.id, schema.relation.subjectId))
    .where(
      and(
        eq(schema.relation.verb, CONTACT),
        eq(schema.relation.resourceKind, 'person'),
        eq(schema.relation.subjectKind, 'person'),
        ne(schema.relation.subjectId, personId),
        inArray(
          schema.relation.resourceId,
          others.filter((row) => row.held).map((row) => row.personId),
        ),
      ),
    );
  const holderOf = new Map(holders.map((row) => [row.heldId, row.holderName]));

  const out: Proposal[] = [];
  for (const row of rows) {
    const left = byIdentity.get(row.identityA);
    const right = byIdentity.get(row.identityB);
    const other = left?.personId === personId ? right : left;
    if (!other || other.personId === personId || other.mergedInto !== null) continue;
    out.push({
      id: row.id,
      subject: row.subject,
      holderName: holderOf.get(other.personId) ?? null,
      heldName: other.displayName,
      heldPersonId: other.personId,
    });
  }
  return out;
}

/** What the claim page shows before anyone commits to anything. */
export interface Invitation {
  linkId: number;
  heldPersonId: number;
  heldName: string;
  handle: string | null;
  inviterName: string | null;
}

export async function invitationDetail(
  personId: number,
  createdBy: number | null,
): Promise<Omit<Invitation, 'linkId'> | null> {
  const db = database();
  const rows = await db
    .select({
      heldPersonId: schema.person.id,
      heldName: schema.person.displayName,
      mergedInto: schema.person.mergedInto,
    })
    .from(schema.person)
    .where(eq(schema.person.id, personId))
    .limit(1);
  const held = rows[0];
  if (!held || held.mergedInto !== null) return null;

  const handles = await db
    .select({ subject: schema.identity.subject })
    .from(schema.identity)
    .where(and(eq(schema.identity.personId, personId), eq(schema.identity.source, 'handle')))
    .orderBy(schema.identity.id)
    .limit(1);

  const inviter = createdBy
    ? await db
        .select({ displayName: schema.person.displayName })
        .from(schema.person)
        .where(eq(schema.person.id, createdBy))
        .limit(1)
    : [];

  return {
    heldPersonId: held.heldPersonId,
    heldName: held.heldName,
    handle: handles[0]?.subject ?? null,
    inviterName: inviter[0]?.displayName ?? null,
  };
}

/** Whether an address is one this person could sign in with, used by
 *  RemoveIdentity to refuse the last door. */
export function countSignInIdentities(personId: number) {
  return database()
    .select({ n: sql<number>`count(*)::int` })
    .from(schema.identity)
    .where(
      and(
        eq(schema.identity.personId, personId),
        inArray(schema.identity.source, ['local', 'oidc']),
      ),
    );
}
