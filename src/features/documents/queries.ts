/* What the document pages read. Mine and shared-with-me are one query over
 * the expanded subject set, not two lists stitched together: the difference
 * between them is which word the row prints. */

import { and, desc, eq, inArray, isNull, or } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';
import { partyNames } from '@/features/groups/queries';
import {
  LEVELS,
  heldRank,
  subjectClause,
  subjectsOf,
  type Actor,
  type Subject,
} from '@/lib/authority';

export interface DocumentRow {
  id: number;
  title: string;
  slug: string | null;
  ownerPartyId: number;
  ownerName: string;
  publishedAt: Date | null;
  updatedAt: Date;
  /* `owner`, or the level a share granted. A word, printed as it is. */
  level: string;
}

/** Everything a member can reach: what they own, what their organizations
 *  own, and what has been shared with any subject they expand to. */
export async function listDocuments(actor: Actor): Promise<DocumentRow[]> {
  return database().transaction(async (tx) => {
    const subjects = await subjectsOf(tx, actor.personId);
    const parties = subjects.filter((row) => row.kind !== 'group').map((row) => row.id);

    const shared = await tx
      .select({ id: schema.relation.resourceId })
      .from(schema.relation)
      .where(
        and(
          eq(schema.relation.resourceKind, 'document'),
          inArray(schema.relation.verb, [...LEVELS]),
          subjectClause(subjects),
        ),
      );
    const sharedIds = [...new Set(shared.map((row) => row.id))];

    const clauses = [inArray(schema.document.ownerPartyId, parties)];
    if (sharedIds.length > 0) clauses.push(inArray(schema.document.id, sharedIds));

    const rows = await tx
      .select({
        id: schema.document.id,
        title: schema.document.title,
        slug: schema.document.slug,
        ownerPartyId: schema.document.ownerPartyId,
        publishedAt: schema.document.publishedAt,
        updatedAt: schema.document.updatedAt,
      })
      .from(schema.document)
      .where(or(...clauses))
      .orderBy(desc(schema.document.updatedAt));

    const owners = await partyNames(tx, rows.map((row) => row.ownerPartyId));
    return Promise.all(
      rows.map(async (row) => ({
        ...row,
        ownerName: owners.get(row.ownerPartyId) ?? 'Somebody',
        level: (await heldRank(tx, actor, 'document', row.id)) ?? 'viewer',
      })),
    );
  });
}

/** The documents an organization owns. */
export async function listOrganizationDocuments(organizationId: number): Promise<DocumentRow[]> {
  return database().transaction(async (tx) => {
    const rows = await tx
      .select({
        id: schema.document.id,
        title: schema.document.title,
        slug: schema.document.slug,
        ownerPartyId: schema.document.ownerPartyId,
        publishedAt: schema.document.publishedAt,
        updatedAt: schema.document.updatedAt,
      })
      .from(schema.document)
      .where(eq(schema.document.ownerPartyId, organizationId))
      .orderBy(desc(schema.document.updatedAt));
    const owners = await partyNames(tx, [organizationId]);
    return rows.map((row) => ({
      ...row,
      ownerName: owners.get(organizationId) ?? 'Somebody',
      level: 'owner',
    }));
  });
}

export interface DocumentDetail extends DocumentRow {
  body: string;
  shares: ShareRow[];
}

export interface ShareRow {
  subject: Subject;
  displayName: string;
  level: string;
}

/** One document, with what the asker may do to it — or null, which the page
 *  reads as the same 404 a document that does not exist gets. */
export async function documentDetail(actor: Actor, id: number): Promise<DocumentDetail | null> {
  return database().transaction(async (tx) => {
    const rows = await tx
      .select({
        id: schema.document.id,
        title: schema.document.title,
        body: schema.document.body,
        slug: schema.document.slug,
        ownerPartyId: schema.document.ownerPartyId,
        publishedAt: schema.document.publishedAt,
        updatedAt: schema.document.updatedAt,
      })
      .from(schema.document)
      .where(eq(schema.document.id, id))
      .limit(1);
    const row = rows[0];
    if (!row) return null;

    const level = await heldRank(tx, actor, 'document', id);
    if (level === null) return null;

    const owners = await partyNames(tx, [row.ownerPartyId]);
    return {
      ...row,
      ownerName: owners.get(row.ownerPartyId) ?? 'Somebody',
      level,
      shares: level === 'owner' ? await listShares(tx, id) : [],
    };
  });
}

/** Who a document is shared with, and at what level. */
export async function listShares(tx: Transaction, documentId: number): Promise<ShareRow[]> {
  const rows = await tx
    .select({
      subjectKind: schema.relation.subjectKind,
      subjectId: schema.relation.subjectId,
      level: schema.relation.verb,
    })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.resourceKind, 'document'),
        eq(schema.relation.resourceId, documentId),
        inArray(schema.relation.verb, [...LEVELS]),
      ),
    );
  const partyIds = rows
    .filter((row) => row.subjectKind !== 'group')
    .map((row) => row.subjectId);
  const groupIds = rows.filter((row) => row.subjectKind === 'group').map((row) => row.subjectId);
  const names = await partyNames(tx, partyIds);
  const groups = new Map<number, string>();
  if (groupIds.length > 0) {
    for (const row of await tx
      .select({ id: schema.group.id, name: schema.group.name })
      .from(schema.group)
      .where(inArray(schema.group.id, groupIds))) {
      groups.set(row.id, row.name);
    }
  }
  return rows.map((row) => ({
    subject: { kind: row.subjectKind as Subject['kind'], id: row.subjectId },
    displayName:
      row.subjectKind === 'group'
        ? (groups.get(row.subjectId) ?? 'A group')
        : (names.get(row.subjectId) ?? 'Somebody'),
    level: row.level,
  }));
}

/** The published document behind a slug, for anybody at all. */
export async function publishedDocument(
  slug: string,
): Promise<{ title: string; body: string; ownerName: string; publishedAt: Date } | null> {
  return database().transaction(async (tx) => {
    const rows = await tx
      .select({
        title: schema.document.title,
        body: schema.document.body,
        ownerPartyId: schema.document.ownerPartyId,
        publishedAt: schema.document.publishedAt,
      })
      .from(schema.document)
      .where(eq(schema.document.slug, slug))
      .limit(1);
    const row = rows[0];
    if (!row || row.publishedAt === null) return null;
    const owners = await partyNames(tx, [row.ownerPartyId]);
    return {
      title: row.title,
      body: row.body,
      ownerName: owners.get(row.ownerPartyId) ?? 'Ronit Nath',
      publishedAt: row.publishedAt,
    };
  });
}

/** Every person and group a member could sensibly share with: the people they
 *  hold or have met, and the groups they are in. */
export async function shareCandidates(
  personId: number,
): Promise<{ subject: Subject; displayName: string }[]> {
  return database().transaction(async (tx) => {
    const subjects = await subjectsOf(tx, personId);
    const groupIds = subjects.filter((row) => row.kind === 'group').map((row) => row.id);
    const orgIds = subjects.filter((row) => row.kind === 'organization').map((row) => row.id);

    const people = await tx
      .select({ id: schema.person.id, name: schema.person.displayName })
      .from(schema.relation)
      .innerJoin(schema.person, eq(schema.person.id, schema.relation.resourceId))
      .where(
        and(
          eq(schema.relation.subjectKind, 'person'),
          eq(schema.relation.subjectId, personId),
          eq(schema.relation.verb, 'contact'),
          eq(schema.relation.resourceKind, 'person'),
          isNull(schema.person.mergedInto),
        ),
      );
    const groups =
      groupIds.length === 0
        ? []
        : await tx
            .select({ id: schema.group.id, name: schema.group.name })
            .from(schema.group)
            .where(inArray(schema.group.id, groupIds));
    const organizations =
      orgIds.length === 0
        ? []
        : await tx
            .select({ id: schema.organization.id, name: schema.organization.name })
            .from(schema.organization)
            .where(inArray(schema.organization.id, orgIds));

    return [
      ...people.map((row) => ({
        subject: { kind: 'person' as const, id: row.id },
        displayName: row.name,
      })),
      ...groups.map((row) => ({
        subject: { kind: 'group' as const, id: row.id },
        displayName: row.name,
      })),
      ...organizations.map((row) => ({
        subject: { kind: 'organization' as const, id: row.id },
        displayName: row.name,
      })),
    ];
  });
}
