/* Reads for the two surfaces. Separate from actions.ts because everything a
 * `'use server'` module exports is a callable endpoint, and a query does not
 * need to be one.
 *
 * The guest read is the interesting one: it is handed a viewer and returns
 * only what that viewer has earned. The address and the details are not
 * blurred in CSS and hidden in the HTML — they are absent from the object
 * until the answer is yes, because a page a stranger can read the source of
 * is a page whose secrets have to be missing rather than styled. */

import { and, asc, desc, eq, inArray, isNull, ne, sql } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import { CONTACT } from '@/features/people/authority';
import { headcount, type Headcount } from './capacity';
import { linkState, type LinkState } from '@/features/people/invitations';
import { orderGuests, type Ordered } from './ordering';

export interface EventSummary {
  id: number;
  slug: string;
  title: string;
  startsAt: Date;
  endsAt: Date | null;
  timezone: string;
  location: string | null;
  capacity: number | null;
  publishedAt: Date | null;
  counts: Headcount;
  invited: number;
}

/** The host's list. Draft and published are words, and the counts are the
 *  only number a host wants from a list row. */
export async function listEvents(personId: number): Promise<EventSummary[]> {
  const db = database();
  const events = await db
    .select({
      id: schema.event.id,
      slug: schema.event.slug,
      title: schema.event.title,
      startsAt: schema.event.startsAt,
      endsAt: schema.event.endsAt,
      timezone: schema.event.timezone,
      location: schema.event.location,
      capacity: schema.event.capacity,
      publishedAt: schema.event.publishedAt,
    })
    .from(schema.event)
    .where(eq(schema.event.hostPersonId, personId))
    .orderBy(desc(schema.event.startsAt));
  if (events.length === 0) return [];

  const ids = events.map((row) => row.id);
  const answers = await db
    .select({
      eventId: schema.rsvp.eventId,
      response: schema.rsvp.response,
      plusOne: schema.rsvp.plusOne,
    })
    .from(schema.rsvp)
    .where(inArray(schema.rsvp.eventId, ids));
  const invites = await db
    .select({ eventId: schema.eventInvite.eventId, n: sql<number>`count(*)::int` })
    .from(schema.eventInvite)
    .where(inArray(schema.eventInvite.eventId, ids))
    .groupBy(schema.eventInvite.eventId);
  const invitedOf = new Map(invites.map((row) => [row.eventId, row.n]));

  return events.map((row) => ({
    ...row,
    counts: headcount(answers.filter((answer) => answer.eventId === row.id)),
    invited: invitedOf.get(row.id) ?? 0,
  }));
}

export interface InviteRow {
  inviteId: number;
  personId: number;
  displayName: string;
  handle: string | null;
  kind: 'personal' | 'open';
  plusOneAllowed: boolean;
  link: { id: number; state: LinkState } | null;
  response: 'yes' | 'maybe' | 'no' | null;
  plusOne: number;
  note: string | null;
  answeredAt: Date | null;
}

/** Everyone the host asked, with the state of their link and their answer.
 *  One query's worth of rows: this is the host's whole page below the copy. */
export async function listInvites(eventId: number): Promise<InviteRow[]> {
  const db = database();
  const rows = await db
    .select({
      inviteId: schema.eventInvite.id,
      personId: schema.eventInvite.personId,
      kind: schema.eventInvite.kind,
      plusOneAllowed: schema.eventInvite.plusOneAllowed,
      displayName: schema.person.displayName,
      linkId: schema.link.id,
      openedAt: schema.link.openedAt,
      claimedAt: schema.link.claimedAt,
      revokedAt: schema.link.revokedAt,
      expiresAt: schema.link.expiresAt,
      response: schema.rsvp.response,
      plusOne: schema.rsvp.plusOne,
      note: schema.rsvp.note,
      answeredAt: schema.rsvp.answeredAt,
    })
    .from(schema.eventInvite)
    .innerJoin(schema.person, eq(schema.person.id, schema.eventInvite.personId))
    .leftJoin(schema.link, eq(schema.link.id, schema.eventInvite.linkId))
    .leftJoin(
      schema.rsvp,
      and(
        eq(schema.rsvp.eventId, schema.eventInvite.eventId),
        eq(schema.rsvp.personId, schema.eventInvite.personId),
      ),
    )
    .where(and(eq(schema.eventInvite.eventId, eventId), isNull(schema.person.mergedInto)))
    .orderBy(asc(schema.eventInvite.id));

  const ids = rows.map((row) => row.personId).filter((id): id is number => id !== null);
  const handles =
    ids.length === 0
      ? []
      : await db
          .select({ personId: schema.identity.personId, subject: schema.identity.subject })
          .from(schema.identity)
          .where(and(eq(schema.identity.source, 'handle'), inArray(schema.identity.personId, ids)))
          .orderBy(schema.identity.id);
  const handleOf = new Map<number, string>();
  for (const row of handles) if (!handleOf.has(row.personId)) handleOf.set(row.personId, row.subject);

  return rows
    .filter((row): row is typeof row & { personId: number } => row.personId !== null)
    .map((row) => ({
      inviteId: row.inviteId,
      personId: row.personId,
      displayName: row.displayName,
      handle: handleOf.get(row.personId) ?? null,
      kind: row.kind,
      plusOneAllowed: row.plusOneAllowed,
      link:
        row.linkId === null
          ? null
          : {
              id: row.linkId,
              state: linkState({
                openedAt: row.openedAt,
                claimedAt: row.claimedAt,
                revokedAt: row.revokedAt,
                expiresAt: row.expiresAt,
              }),
            },
      response: row.response,
      plusOne: row.plusOne ?? 0,
      note: row.note,
      answeredAt: row.answeredAt,
    }));
}

/** The event's open link, if it has one and it still works. */
export async function openLinkState(eventId: number): Promise<LinkState | null> {
  const rows = await database()
    .select({
      openedAt: schema.link.openedAt,
      claimedAt: schema.link.claimedAt,
      revokedAt: schema.link.revokedAt,
      expiresAt: schema.link.expiresAt,
    })
    .from(schema.link)
    .where(
      and(
        eq(schema.link.kind, 'event_open'),
        eq(schema.link.targetKind, 'event'),
        eq(schema.link.targetId, eventId),
      ),
    )
    .orderBy(desc(schema.link.id))
    .limit(1);
  const row = rows[0];
  return row ? linkState(row) : null;
}

export interface GuestList {
  shown: Ordered[];
  more: number;
  total: number;
}

/** Who's coming, in the viewer's order. `sharedWith` is the R5 hook: today
 *  it is answered from `relation` contact edges — people the host wrote down
 *  who the viewer also shares an edge with — and R5 replaces the body of
 *  `sharedCircle` with a groups query without touching this. */
export async function guestList(eventId: number, viewerId: number | null, limit: number): Promise<GuestList> {
  const db = database();
  const rows = await db
    .select({
      personId: schema.rsvp.personId,
      displayName: schema.person.displayName,
      answeredAt: schema.rsvp.answeredAt,
      plusOne: schema.rsvp.plusOne,
    })
    .from(schema.rsvp)
    .innerJoin(schema.person, eq(schema.person.id, schema.rsvp.personId))
    .where(
      and(
        eq(schema.rsvp.eventId, eventId),
        eq(schema.rsvp.response, 'yes'),
        isNull(schema.person.mergedInto),
        viewerId === null ? undefined : ne(schema.rsvp.personId, viewerId),
      ),
    )
    .orderBy(asc(schema.rsvp.answeredAt));

  const shared = viewerId === null ? new Set<number>() : await sharedCircle(viewerId, rows.map((r) => r.personId));
  const ordered = orderGuests(rows, shared);
  return {
    shown: ordered.slice(0, limit),
    more: Math.max(0, ordered.length - limit),
    total: ordered.length,
  };
}

/** The R5 seam. A circle is a group and groups do not exist yet, so the
 *  closest true statement this rung can make is "somebody wrote both of you
 *  down": a `contact` edge from a person who also holds the viewer. */
export async function sharedCircle(
  viewerId: number,
  candidates: readonly number[],
): Promise<Set<number>> {
  if (candidates.length === 0) return new Set();
  const db = database();
  const holders = db
    .select({ id: schema.relation.subjectId })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.verb, CONTACT),
        eq(schema.relation.resourceKind, 'person'),
        eq(schema.relation.resourceId, viewerId),
      ),
    );
  const rows = await db
    .select({ id: schema.relation.resourceId })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.verb, CONTACT),
        eq(schema.relation.resourceKind, 'person'),
        inArray(schema.relation.resourceId, [...candidates]),
        inArray(schema.relation.subjectId, holders),
      ),
    );
  return new Set(rows.map((row) => row.id));
}

export interface ViewerAnswer {
  response: 'yes' | 'maybe' | 'no';
  plusOne: number;
  note: string | null;
}

export async function answerOf(eventId: number, personId: number): Promise<ViewerAnswer | null> {
  const rows = await database()
    .select({
      response: schema.rsvp.response,
      plusOne: schema.rsvp.plusOne,
      note: schema.rsvp.note,
    })
    .from(schema.rsvp)
    .where(and(eq(schema.rsvp.eventId, eventId), eq(schema.rsvp.personId, personId)))
    .limit(1);
  return rows[0] ?? null;
}

/** The rows capacity is counted from. */
export async function answers(eventId: number) {
  return database()
    .select({ response: schema.rsvp.response, plusOne: schema.rsvp.plusOne })
    .from(schema.rsvp)
    .where(eq(schema.rsvp.eventId, eventId));
}

export async function hostName(personId: number): Promise<string | null> {
  const rows = await database()
    .select({ displayName: schema.person.displayName })
    .from(schema.person)
    .where(eq(schema.person.id, personId))
    .limit(1);
  return rows[0]?.displayName ?? null;
}
