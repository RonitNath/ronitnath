/* What the group pages read, and the one question another feature asks of
 * this one: who shares a circle with whom (src/features/events/ordering.ts). */

import { and, asc, eq, inArray, isNull, or } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';
import {
  ROLES,
  expandSubjects,
  heldRank,
  membershipEdges,
  subjectsOf,
  type Role,
  type Subject,
} from '@/lib/authority';

export interface GroupRow {
  id: number;
  name: string;
  ownerPartyId: number;
  ownerName: string;
  role: Role;
  members: number;
}

async function decorate(
  tx: Transaction,
  personId: number,
  rows: { id: number; name: string; ownerPartyId: number }[],
): Promise<GroupRow[]> {
  if (rows.length === 0) return [];
  const ids = rows.map((row) => row.id);
  const counts = await tx
    .select({ id: schema.relation.resourceId, subjectId: schema.relation.subjectId })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.resourceKind, 'group'),
        inArray(schema.relation.resourceId, ids),
        inArray(schema.relation.verb, [...ROLES]),
      ),
    );
  const owners = await partyNames(tx, rows.map((row) => row.ownerPartyId));

  return Promise.all(
    rows.map(async (row) => ({
      ...row,
      ownerName: owners.get(row.ownerPartyId) ?? 'Somebody',
      members: counts.filter((count) => count.id === row.id).length,
      role: ((await heldRank(tx, { personId, isOperator: false }, 'group', row.id)) ??
        'member') as Role,
    })),
  );
}

/** What a party is called, whoever it is. */
export async function partyNames(
  tx: Transaction,
  ids: readonly number[],
): Promise<Map<number, string>> {
  const out = new Map<number, string>();
  if (ids.length === 0) return out;
  const unique = [...new Set(ids)];
  for (const row of await tx
    .select({ id: schema.person.id, name: schema.person.displayName })
    .from(schema.person)
    .where(inArray(schema.person.id, unique))) {
    out.set(row.id, row.name);
  }
  for (const row of await tx
    .select({ id: schema.organization.id, name: schema.organization.name })
    .from(schema.organization)
    .where(inArray(schema.organization.id, unique))) {
    out.set(row.id, row.name);
  }
  return out;
}

/** The groups a member owns or is in — including the ones they reach through
 *  another group, and the ones their organizations own. */
export async function listGroups(personId: number): Promise<GroupRow[]> {
  return database().transaction(async (tx) => {
    const subjects = await subjectsOf(tx, personId);
    const parties = subjects.filter((row) => row.kind !== 'group').map((row) => row.id);
    const groups = subjects.filter((row) => row.kind === 'group').map((row) => row.id);
    const clauses = [inArray(schema.group.ownerPartyId, parties)];
    if (groups.length > 0) clauses.push(inArray(schema.group.id, groups));

    const rows = await tx
      .select({
        id: schema.group.id,
        name: schema.group.name,
        ownerPartyId: schema.group.ownerPartyId,
      })
      .from(schema.group)
      .where(or(...clauses))
      .orderBy(asc(schema.group.name));
    return decorate(tx, personId, rows);
  });
}

/** The groups an organization owns. */
export async function listOrganizationGroups(
  organizationId: number,
  personId: number,
): Promise<GroupRow[]> {
  return database().transaction(async (tx) => {
    const rows = await tx
      .select({
        id: schema.group.id,
        name: schema.group.name,
        ownerPartyId: schema.group.ownerPartyId,
      })
      .from(schema.group)
      .where(eq(schema.group.ownerPartyId, organizationId))
      .orderBy(asc(schema.group.name));
    return decorate(tx, personId, rows);
  });
}

export interface GroupMember {
  subject: Subject;
  displayName: string;
  held: boolean;
  role: Role;
}

/** Who is in a group, directly: people and groups, in one list, because that
 *  is what the group is. */
export async function listGroupMembers(groupId: number): Promise<GroupMember[]> {
  return database().transaction(async (tx) => {
    const rows = await tx
      .select({
        subjectKind: schema.relation.subjectKind,
        subjectId: schema.relation.subjectId,
        role: schema.relation.verb,
      })
      .from(schema.relation)
      .where(
        and(
          eq(schema.relation.resourceKind, 'group'),
          eq(schema.relation.resourceId, groupId),
          inArray(schema.relation.verb, [...ROLES]),
        ),
      );

    const personIds = rows.filter((row) => row.subjectKind === 'person').map((row) => row.subjectId);
    const groupIds = rows.filter((row) => row.subjectKind === 'group').map((row) => row.subjectId);
    const people = new Map<number, { name: string; held: boolean }>();
    if (personIds.length > 0) {
      for (const row of await tx
        .select({
          id: schema.person.id,
          name: schema.person.displayName,
          held: schema.person.held,
        })
        .from(schema.person)
        .where(and(inArray(schema.person.id, personIds), isNull(schema.person.mergedInto)))) {
        people.set(row.id, { name: row.name, held: row.held });
      }
    }
    const groups = new Map<number, string>();
    if (groupIds.length > 0) {
      for (const row of await tx
        .select({ id: schema.group.id, name: schema.group.name })
        .from(schema.group)
        .where(inArray(schema.group.id, groupIds))) {
        groups.set(row.id, row.name);
      }
    }

    const out: GroupMember[] = [];
    for (const row of rows) {
      const subject: Subject = {
        kind: row.subjectKind === 'group' ? 'group' : 'person',
        id: row.subjectId,
      };
      if (subject.kind === 'person') {
        const person = people.get(subject.id);
        if (!person) continue;
        out.push({ subject, displayName: person.name, held: person.held, role: row.role as Role });
      } else {
        const name = groups.get(subject.id);
        if (!name) continue;
        out.push({ subject, displayName: name, held: false, role: row.role as Role });
      }
    }
    return out.sort((a, b) => a.displayName.localeCompare(b.displayName));
  });
}

/** Which of these people share a group with the viewer. This is the R4 hook
 *  made real: a circle is a group, and two people share one when the viewer's
 *  expanded subjects and theirs meet in a group. */
export async function sharedGroupMembers(
  viewerId: number,
  candidates: readonly number[],
): Promise<Set<number>> {
  if (candidates.length === 0) return new Set();
  return database().transaction(async (tx) => {
    const edges = await membershipEdges(tx);
    const mine = new Set(
      expandSubjects({ kind: 'person', id: viewerId }, edges)
        .filter((row) => row.kind === 'group')
        .map((row) => row.id),
    );
    if (mine.size === 0) return new Set<number>();
    const out = new Set<number>();
    for (const candidate of candidates) {
      const theirs = expandSubjects({ kind: 'person', id: candidate }, edges);
      if (theirs.some((row) => row.kind === 'group' && mine.has(row.id))) out.add(candidate);
    }
    return out;
  });
}
