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
import { sharedGroupMembers } from '@/features/groups/queries';
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

/** Who's coming, in the viewer's order. The shared-circle question is asked
 *  of the groups feature and answered there; this stays as R4 wrote it. */
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

/** The seam, filled. A circle is a group, and R5 made groups real: two people
 *  share one when the viewer's expanded subjects and theirs meet in a group,
 *  however many groups deep either of them sits. Nothing else counts — being
 *  written down by the same person is an address book, not a circle. */
export async function sharedCircle(
  viewerId: number,
  candidates: readonly number[],
): Promise<Set<number>> {
  return sharedGroupMembers(viewerId, candidates);
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

export interface PhotoRow {
  id: number;
  personId: number | null;
  addedBy: string | null;
  mimeType: string;
  width: number;
  height: number;
  hiddenAt: Date | null;
  createdAt: Date;
}

/* The pictures of one event, newest first. The bytes are not in the select:
 * `bytea` is stored out of line, and a gallery of twelve photographs would be
 * fifty megabytes of Postgres traffic to draw twelve `<img>` tags that are
 * each going to be fetched anyway.
 *
 * `hidden` is the host's own view. A guest is given the visible ones only —
 * absent from the HTML rather than styled out of it, because the host's act of
 * taking a picture down has to mean the bytes stop being named in anybody's
 * page. */
export async function photosOf(
  eventId: number,
  options: { hidden?: boolean } = {},
): Promise<PhotoRow[]> {
  const where = options.hidden
    ? eq(schema.photo.eventId, eventId)
    : and(eq(schema.photo.eventId, eventId), isNull(schema.photo.hiddenAt));
  return database()
    .select({
      id: schema.photo.id,
      personId: schema.photo.personId,
      addedBy: schema.person.displayName,
      mimeType: schema.photo.mimeType,
      width: schema.photo.width,
      height: schema.photo.height,
      hiddenAt: schema.photo.hiddenAt,
      createdAt: schema.photo.createdAt,
    })
    .from(schema.photo)
    .leftJoin(schema.person, eq(schema.person.id, schema.photo.personId))
    .where(where)
    .orderBy(desc(schema.photo.createdAt), desc(schema.photo.id));
}

/** One picture's bytes, for the route that serves them. Null for a hidden
 *  picture as well as for one that was never there: once the host takes it
 *  down, the URL that was in somebody's history stops answering.
 *
 *  The host is the exception, and has to be: the page where a picture is put
 *  back has to show which picture it is. */
export async function photoBytes(
  eventId: number,
  photoId: number,
  options: { hidden?: boolean } = {},
): Promise<{ bytes: Buffer; mimeType: string } | null> {
  const clauses = [eq(schema.photo.id, photoId), eq(schema.photo.eventId, eventId)];
  if (!options.hidden) clauses.push(isNull(schema.photo.hiddenAt));
  const rows = await database()
    .select({ bytes: schema.photo.bytes, mimeType: schema.photo.mimeType })
    .from(schema.photo)
    .where(and(...clauses))
    .limit(1);
  return rows[0] ?? null;
}
