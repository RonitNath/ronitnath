/* allows(tx, actor, want) for events. Two kinds of caller reach this feature
 * and they are authorised in two different ways.
 *
 * A **host command** is a member's: the actor is the event's host, or a
 * platform operator. Anything else — somebody else's event, an event that
 * does not exist — is the same decline, because the difference is the fact a
 * stranger would like to learn.
 *
 * A **guest command** carries no session at all. What authorises it is the
 * bearer link in the URL, resolved in src/features/events/links.ts, plus the
 * event being published: an unpublished event declines every link it ever
 * minted, exactly as a link that never existed does. */

import { and, eq } from 'drizzle-orm';

import { schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';

export interface Actor {
  personId: number;
  isOperator: boolean;
}

export const HOST = 'host';
export const INVITED = 'invited';
export const ATTENDING = 'attending';

export interface EventRow {
  id: number;
  hostPersonId: number;
  slug: string;
  title: string;
  summary: string | null;
  startsAt: Date;
  endsAt: Date | null;
  location: string | null;
  address: string | null;
  timezone: string;
  body: string;
  capacity: number | null;
  colour: string | null;
  posterUrl: string | null;
  revealGuests: boolean;
  sequence: number;
  publishedAt: Date | null;
  updatedAt: Date;
  createdAt: Date;
}

const columns = {
  id: schema.event.id,
  hostPersonId: schema.event.hostPersonId,
  slug: schema.event.slug,
  title: schema.event.title,
  summary: schema.event.summary,
  startsAt: schema.event.startsAt,
  endsAt: schema.event.endsAt,
  location: schema.event.location,
  address: schema.event.address,
  timezone: schema.event.timezone,
  body: schema.event.body,
  capacity: schema.event.capacity,
  colour: schema.event.colour,
  posterUrl: schema.event.posterUrl,
  revealGuests: schema.event.revealGuests,
  sequence: schema.event.sequence,
  publishedAt: schema.event.publishedAt,
  updatedAt: schema.event.updatedAt,
  createdAt: schema.event.createdAt,
} as const;

export const EVENT_COLUMNS = columns;

/** The event an actor may act on as its host, or null — one answer for
 *  "not yours" and "not there". */
export async function hostedEvent(
  tx: Transaction,
  actor: Actor,
  eventId: number,
): Promise<EventRow | null> {
  const rows = await tx.select(columns).from(schema.event).where(eq(schema.event.id, eventId)).limit(1);
  const row = rows[0];
  if (!row) return null;
  if (row.hostPersonId === actor.personId || actor.isOperator) return row;
  return null;
}

/** The published event behind a slug, for a guest who has a link. Nothing
 *  unpublished is reachable from outside `/app`. */
export async function publishedEvent(tx: Transaction, slug: string): Promise<EventRow | null> {
  const rows = await tx
    .select(columns)
    .from(schema.event)
    .where(eq(schema.event.slug, slug))
    .limit(1);
  const row = rows[0];
  if (!row || row.publishedAt === null) return null;
  return row;
}

/** The relations an event writes: the host owns it, an invited person is
 *  invited, a yes is attending. They are what R5's groups and R6's operator
 *  surface will read; nothing in R4 authorises off them, because the bearer
 *  link and the host column already say everything a command needs. */
export async function writeRelation(
  tx: Transaction,
  input: { personId: number; verb: string; eventId: number },
): Promise<void> {
  await tx
    .insert(schema.relation)
    .values({
      subjectKind: 'person',
      subjectId: input.personId,
      verb: input.verb,
      resourceKind: 'event',
      resourceId: input.eventId,
    })
    .onConflictDoNothing();
}

export async function dropRelation(
  tx: Transaction,
  input: { personId: number; verb: string; eventId: number },
): Promise<void> {
  await tx
    .delete(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.subjectId, input.personId),
        eq(schema.relation.verb, input.verb),
        eq(schema.relation.resourceKind, 'event'),
        eq(schema.relation.resourceId, input.eventId),
      ),
    );
}
