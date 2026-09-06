'use server';

/* Respond. The one command a stranger may run.
 *
 * Nothing here reads a session for authority. What authorises it is the token
 * in the URL: a personal link says which person is answering, an open link
 * says only which event, and the name typed into the form becomes a held
 * person of the host's — the same kind of row HoldPerson makes, so the guest
 * who signs in later claims it and the host sees one person.
 *
 * A signed-in member who is invited may answer without a link at all; the
 * session is then the bearer.
 *
 * Capacity is not a gate. A yes past the host's number is still a yes: the
 * page says `full` and the host sees where the line was crossed, because an
 * answer refused is an answer lost. */

import { and, eq, isNull, sql } from 'drizzle-orm';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { FormState } from '@/features/auth/form-state';
import type { Transaction } from '@/features/auth/db';
import { createPerson } from '@/features/auth/provision';
import { currentPrincipal } from '@/features/auth/session';
import { CONTACT } from '@/features/people/authority';
import { normalizeHandle } from '@/features/people/handles';
import { ATTENDING, INVITED, dropRelation, publishedEvent, writeRelation } from './authority';
import { fullness } from './capacity';
import { mintPersonalLink, readEventLink } from './links';

/* Expired, revoked, unpublished, never existed: one sentence for all of them. */
const DECLINED = 'This link does not work.';

const answerInput = z.object({
  slug: z.string().trim().min(1).max(80),
  token: z.string().trim().max(200),
  response: z.enum(['yes', 'maybe', 'no']),
  plusOne: z.coerce.number().int().min(0).max(9),
  note: z.string().trim().max(500),
  name: z.string().trim().max(120),
});

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

/** Who is answering, or null. Three ways in, one shape out. */
async function bearer(
  tx: Transaction,
  input: { eventId: number; hostPersonId: number; token: string; name: string; sessionPersonId: number | null },
): Promise<{ personId: number; token: string | null } | null> {
  if (input.token) {
    const link = await readEventLink(tx, input.token);
    if (!link) return null;
    if (link.kind === 'event_personal') {
      /* The link says a person; the invite says that person was asked to
       * this event. A personal link for another event is not a key to this
       * one. */
      const invited = await tx
        .select({ id: schema.eventInvite.id })
        .from(schema.eventInvite)
        .where(
          and(
            eq(schema.eventInvite.eventId, input.eventId),
            eq(schema.eventInvite.personId, link.targetId),
          ),
        )
        .limit(1);
      if (invited.length === 0) return null;
      return { personId: link.targetId, token: null };
    }
    if (link.targetId !== input.eventId) return null;
    /* An open link and a name: a held person of the host's, invited on the
     * spot, with a personal link of their own from here on. */
    const name = input.name.trim();
    if (name.length === 0) return null;
    const handle = normalizeHandle(name);
    const personId = await createPerson(tx, { displayName: name, held: true });
    if (handle) {
      await tx
        .insert(schema.identity)
        .values({ personId, source: 'handle', subject: handle.subject });
    }
    await tx
      .insert(schema.relation)
      .values({
        subjectKind: 'person',
        subjectId: input.hostPersonId,
        verb: CONTACT,
        resourceKind: 'person',
        resourceId: personId,
      })
      .onConflictDoNothing();
    const { token, linkId } = await mintPersonalLink(tx, {
      personId,
      createdBy: input.hostPersonId,
    });
    await tx
      .insert(schema.eventInvite)
      .values({ eventId: input.eventId, personId, kind: 'open', linkId })
      .onConflictDoNothing();
    await writeRelation(tx, { personId, verb: INVITED, eventId: input.eventId });
    return { personId, token };
  }

  if (input.sessionPersonId === null) return null;
  const invited = await tx
    .select({ id: schema.eventInvite.id })
    .from(schema.eventInvite)
    .where(
      and(
        eq(schema.eventInvite.eventId, input.eventId),
        eq(schema.eventInvite.personId, input.sessionPersonId),
      ),
    )
    .limit(1);
  if (invited.length === 0 && input.hostPersonId !== input.sessionPersonId) return null;
  return { personId: input.sessionPersonId, token: null };
}

/** Respond, with the plus-one and the note folded in: one row per person per
 *  event, upserted, so changing your mind is the same command as answering. */
export async function respond(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = answerInput.safeParse({
    slug: field(form, 'slug'),
    token: field(form, 'token'),
    response: field(form, 'response'),
    plusOne: field(form, 'plusOne') || '0',
    note: field(form, 'note'),
    name: field(form, 'name'),
  });
  if (!parsed.success) return { error: DECLINED };
  const input = parsed.data;
  const principal = await currentPrincipal();

  const outcome = await database().transaction(async (tx) => {
    const event = await publishedEvent(tx, input.slug);
    if (!event) return 'declined' as const;
    const who = await bearer(tx, {
      eventId: event.id,
      hostPersonId: event.hostPersonId,
      token: input.token,
      name: input.name,
      sessionPersonId: principal?.personId ?? null,
    });
    if (!who) return 'declined' as const;

    const rows = await tx
      .select({ response: schema.rsvp.response, plusOne: schema.rsvp.plusOne, personId: schema.rsvp.personId })
      .from(schema.rsvp)
      .innerJoin(schema.person, eq(schema.person.id, schema.rsvp.personId))
      .where(and(eq(schema.rsvp.eventId, event.id), isNull(schema.person.mergedInto)));
    const others = rows.filter((row) => row.personId !== who.personId);
    const room = fullness(event.capacity, others, {
      response: input.response,
      plusOne: input.plusOne,
    });

    /* A plus-one the host did not allow is one person answering, not an
     * error: the number is clamped rather than the answer refused. */
    const allowed = await tx
      .select({ plusOneAllowed: schema.eventInvite.plusOneAllowed })
      .from(schema.eventInvite)
      .where(
        and(eq(schema.eventInvite.eventId, event.id), eq(schema.eventInvite.personId, who.personId)),
      )
      .limit(1);
    const plusOne = allowed[0]?.plusOneAllowed === false ? 0 : input.plusOne;

    await tx
      .insert(schema.rsvp)
      .values({
        eventId: event.id,
        personId: who.personId,
        response: input.response,
        plusOne,
        note: input.note || null,
      })
      .onConflictDoUpdate({
        target: [schema.rsvp.eventId, schema.rsvp.personId],
        set: {
          response: input.response,
          plusOne,
          note: input.note || null,
          answeredAt: sql`now()`,
        },
      });

    if (input.response === 'yes') {
      await writeRelation(tx, { personId: who.personId, verb: ATTENDING, eventId: event.id });
    } else {
      await dropRelation(tx, { personId: who.personId, verb: ATTENDING, eventId: event.id });
    }
    await recordAudit(tx, {
      actorPersonId: who.personId,
      command: 'respond',
      targetKind: 'event',
      targetId: event.id,
      payload: { response: input.response, plusOne, via: input.token ? 'link' : 'session', room },
    });
    return { room, own: who.token } as const;
  });

  if (outcome === 'declined') return { error: DECLINED };
  /* The answer lands and the page is drawn again from the database: the
   * address appears, the list sharpens, the chosen word is chosen. A notice
   * rendered beside the form would be a second account of the same fact, and
   * one that a re-render is free to lose — so the page is the answer.
   *
   * A stranger who came through the open link now has a link of their own,
   * and the address bar becomes it: coming back is coming back as themselves
   * rather than as another new guest. */
  const carry = outcome.own ?? input.token;
  redirect(carry ? `/e/${input.slug}?l=${carry}` : `/e/${input.slug}`);
}
