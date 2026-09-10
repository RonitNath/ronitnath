'use server';

/* The host's commands. Each is one transaction with its audit row inside it,
 * zod at the door, and one uniform decline: an event that is not yours and an
 * event that is not there are the same answer.
 *
 * The guest's command lives in respond.ts, because it is authorised by a
 * bearer link rather than by a session and mixing the two in one file is how
 * a guard gets forgotten. */

import { and, eq, isNull, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { FormState } from '@/features/auth/form-state';
import { createPerson } from '@/features/people/provision';
import { currentPrincipal } from '@/features/auth/principal';
import { CONTACT } from '@/features/people/authority';
import { HANDLE_MAX, normalizeHandle } from '@/features/people/handles';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { HOST, INVITED, dropRelation, hostedEvent, writeRelation } from './authority';
import { guestUrl, mintOpenLink, mintPersonalLink } from './links';
import { slugCandidate } from './slug';
import { fromWallClock, isZone } from './time';

const NO_SUCH = 'That is not something you can do here.';
const EVENTS = '/app/events';

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

async function actor() {
  const principal = await currentPrincipal();
  if (!principal) redirect('/auth/sign-in');
  return { personId: principal.personId, isOperator: principal.isOperator };
}

const copy = {
  title: z.string().trim().min(1).max(140),
  location: z.string().trim().max(200),
  address: z.string().trim().max(400),
  body: z.string().max(8000),
  timezone: z.string().trim().max(64).refine(isZone),
  startsAt: z.string().trim().min(1),
  endsAt: z.string().trim(),
  capacity: z.string().trim(),
  colour: z.string().trim().max(40),
  posterUrl: z.string().trim().max(500),
};

const createInput = z.object({
  title: copy.title,
  timezone: copy.timezone,
  startsAt: copy.startsAt,
  endsAt: copy.endsAt,
  location: copy.location,
  address: copy.address,
  body: copy.body,
  capacity: copy.capacity,
});

const updateInput = createInput.extend({
  colour: copy.colour,
  posterUrl: copy.posterUrl,
  revealGuests: z.boolean(),
});

function capacityOf(raw: string): number | null | undefined {
  if (raw === '') return null;
  const n = Number(raw);
  if (!Number.isInteger(n) || n < 1 || n > 10_000) return undefined;
  return n;
}

function readForm(form: FormData) {
  return {
    title: field(form, 'title'),
    timezone: field(form, 'timezone') || 'America/Los_Angeles',
    startsAt: field(form, 'startsAt'),
    endsAt: field(form, 'endsAt'),
    location: field(form, 'location'),
    address: field(form, 'address'),
    body: field(form, 'body'),
    capacity: field(form, 'capacity'),
  };
}

const BAD_COPY = 'A title, a start, and a zone the clock knows.';

/** CreateEvent. Created is not published: the event exists, it has a slug,
 *  and nothing outside `/app` can see it yet. */
export async function createEvent(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const parsed = createInput.safeParse(readForm(form));
  if (!parsed.success) return { error: BAD_COPY };
  const input = parsed.data;
  const startsAt = fromWallClock(input.startsAt, input.timezone);
  const endsAt = input.endsAt ? fromWallClock(input.endsAt, input.timezone) : null;
  if (!startsAt) return { error: BAD_COPY };
  if (endsAt && endsAt.getTime() <= startsAt.getTime()) {
    return { error: 'The end comes after the start.' };
  }
  const capacity = capacityOf(input.capacity);
  if (capacity === undefined) return { error: 'A capacity is a whole number of people.' };

  const created = await database().transaction(async (tx) => {
    /* Uniqueness is the index's answer, not a lookup's: two hosts naming the
     * same evening in the same second cannot both win. */
    for (let attempt = 0; attempt < 12; attempt += 1) {
      const slug = slugCandidate(input.title, attempt);
      const rows = await tx
        .insert(schema.event)
        .values({
          hostPersonId: me.personId,
          slug,
          title: input.title,
          startsAt,
          endsAt,
          timezone: input.timezone,
          location: input.location || null,
          address: input.address || null,
          body: input.body,
          capacity,
        })
        .onConflictDoNothing({ target: schema.event.slug })
        .returning({ id: schema.event.id });
      const row = rows[0];
      if (!row) continue;
      await writeRelation(tx, { personId: me.personId, verb: HOST, eventId: row.id });
      await tx
        .insert(schema.resource)
        .values({ kind: 'event', refId: row.id, ownerPartyId: me.personId })
        .onConflictDoNothing();
      await recordAudit(tx, {
        actorPersonId: me.personId,
        command: 'create-event',
        targetKind: 'event',
        targetId: row.id,
        payload: { slug },
      });
      return row.id;
    }
    return null;
  });

  if (created === null) return { error: 'That name is taken too many times over.' };
  revalidatePath(EVENTS);
  redirect(`${EVENTS}/${encodeId('event', created)}`);
}

/** UpdateEvent. A change after publication simply updates the page — nobody
 *  is mailed — and bumps the sequence, so a calendar that already holds the
 *  entry replaces it rather than keeping two. */
export async function updateEvent(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('event', field(form, 'event'));
  if (target === null) return { error: NO_SUCH };
  const parsed = updateInput.safeParse({
    ...readForm(form),
    colour: field(form, 'colour'),
    posterUrl: field(form, 'posterUrl'),
    revealGuests: form.get('revealGuests') === 'on',
  });
  if (!parsed.success) return { error: BAD_COPY };
  const input = parsed.data;
  const startsAt = fromWallClock(input.startsAt, input.timezone);
  const endsAt = input.endsAt ? fromWallClock(input.endsAt, input.timezone) : null;
  if (!startsAt) return { error: BAD_COPY };
  if (endsAt && endsAt.getTime() <= startsAt.getTime()) {
    return { error: 'The end comes after the start.' };
  }
  const capacity = capacityOf(input.capacity);
  if (capacity === undefined) return { error: 'A capacity is a whole number of people.' };

  const ok = await database().transaction(async (tx) => {
    const event = await hostedEvent(tx, me, target);
    if (!event) return false;
    await tx
      .update(schema.event)
      .set({
        title: input.title,
        startsAt,
        endsAt,
        timezone: input.timezone,
        location: input.location || null,
        address: input.address || null,
        body: input.body,
        capacity,
        colour: input.colour || null,
        posterUrl: input.posterUrl || null,
        revealGuests: input.revealGuests,
        updatedAt: sql`now()`,
        /* Only a published event's calendar entry is out there to correct. */
        sequence: event.publishedAt ? event.sequence + 1 : event.sequence,
      })
      .where(eq(schema.event.id, target));
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'update-event',
      targetKind: 'event',
      targetId: target,
      payload: { sequence: event.publishedAt ? event.sequence + 1 : event.sequence },
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(`${EVENTS}/${field(form, 'event')}`);
  return { notice: 'Saved.' };
}

/** PublishEvent. This is where the links are born: one personal link per
 *  invited person, and one open link for the event. The URLs are shown once,
 *  on the page that asked for them, and never stored. */
export async function publishEvent(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('event', field(form, 'event'));
  if (target === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    const event = await hostedEvent(tx, me, target);
    if (!event) return null;
    if (event.publishedAt === null) {
      await tx
        .update(schema.event)
        .set({ publishedAt: sql`now()`, updatedAt: sql`now()` })
        .where(eq(schema.event.id, target));
    }

    /* Everyone invited who has no live link yet gets one. Publishing twice
     * does not mint a second link for the same guest. */
    const waiting = await tx
      .select({
        id: schema.eventInvite.id,
        personId: schema.eventInvite.personId,
        displayName: schema.person.displayName,
      })
      .from(schema.eventInvite)
      .innerJoin(schema.person, eq(schema.person.id, schema.eventInvite.personId))
      .where(and(eq(schema.eventInvite.eventId, target), isNull(schema.eventInvite.linkId)));
    const minted: { name: string; url: string }[] = [];
    for (const invite of waiting) {
      if (invite.personId === null) continue;
      const { token, linkId } = await mintPersonalLink(tx, {
        personId: invite.personId,
        createdBy: me.personId,
      });
      await tx
        .update(schema.eventInvite)
        .set({ linkId })
        .where(eq(schema.eventInvite.id, invite.id));
      minted.push({ name: invite.displayName, url: guestUrl(event.slug, token) });
    }

    const open = await tx
      .select({ id: schema.link.id })
      .from(schema.link)
      .where(
        and(
          eq(schema.link.kind, 'event_open'),
          eq(schema.link.targetKind, 'event'),
          eq(schema.link.targetId, target),
          isNull(schema.link.revokedAt),
        ),
      )
      .limit(1);
    if (open.length === 0) {
      const { token } = await mintOpenLink(tx, { eventId: target, createdBy: me.personId });
      minted.push({ name: 'Anyone', url: guestUrl(event.slug, token) });
    }

    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'publish-event',
      targetKind: 'event',
      targetId: target,
      payload: { minted: minted.length },
    });
    return minted;
  });

  if (ok === null) return { error: NO_SUCH };
  revalidatePath(`${EVENTS}/${field(form, 'event')}`);
  return {
    mintedLinks: ok,
    notice:
      ok.length === 0
        ? 'Published.'
        : 'Published. Copy these now — they are not stored and cannot be shown again.',
  };
}

/** UnpublishEvent. The page stops answering and every link with it — a guest
 *  who follows one is told the link does not work, the same sentence a made-up
 *  token gets. */
export async function unpublishEvent(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('event', field(form, 'event'));
  if (target === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    const event = await hostedEvent(tx, me, target);
    if (!event) return false;
    await tx
      .update(schema.event)
      .set({ publishedAt: null, updatedAt: sql`now()`, sequence: event.sequence + 1 })
      .where(eq(schema.event.id, target));
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'unpublish-event',
      targetKind: 'event',
      targetId: target,
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(`${EVENTS}/${field(form, 'event')}`);
  return { notice: 'Unpublished. The links decline until you publish again.' };
}

/* One handle or name per line: what a host pastes when they are inviting the
 * six people they were already texting. */
const invitesInput = z.object({ people: z.string().max(4000) });

/** InvitePeople. Existing contacts by public id, new ones by handle — a
 *  handle nobody holds becomes a held person, exactly as HoldPerson makes
 *  one, so events and people are the same address book. */
export async function invitePeople(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('event', field(form, 'event'));
  if (target === null) return { error: NO_SUCH };
  const parsed = invitesInput.safeParse({ people: field(form, 'people') });
  if (!parsed.success) return { error: 'One name, address or number per line.' };

  const chosen = form
    .getAll('contact')
    .filter((value): value is string => typeof value === 'string')
    .map((value) => tryDecodeId('person', value))
    .filter((id): id is number => id !== null);
  const typed = parsed.data.people
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && line.length <= HANDLE_MAX);
  if (chosen.length === 0 && typed.length === 0) return { error: 'Nobody to invite.' };

  const added = await database().transaction(async (tx) => {
    const event = await hostedEvent(tx, me, target);
    if (!event) return null;
    const people: number[] = [...chosen];

    for (const line of typed) {
      const handle = normalizeHandle(line);
      if (!handle) continue;
      /* The host's own address book first: inviting somebody twice under the
       * same handle invites them once. */
      const existing = await tx
        .select({ personId: schema.identity.personId })
        .from(schema.identity)
        .innerJoin(
          schema.relation,
          and(
            eq(schema.relation.subjectKind, 'person'),
            eq(schema.relation.subjectId, me.personId),
            eq(schema.relation.verb, CONTACT),
            eq(schema.relation.resourceKind, 'person'),
            eq(schema.relation.resourceId, schema.identity.personId),
          ),
        )
        .innerJoin(schema.person, eq(schema.person.id, schema.identity.personId))
        .where(
          and(
            eq(schema.identity.source, 'handle'),
            eq(schema.identity.subject, handle.subject),
            isNull(schema.person.mergedInto),
          ),
        )
        .limit(1);
      if (existing[0]) {
        people.push(existing[0].personId);
        continue;
      }
      const heldId = await createPerson(tx, { displayName: handle.raw, held: true });
      await tx
        .insert(schema.identity)
        .values({ personId: heldId, source: 'handle', subject: handle.subject });
      await tx
        .insert(schema.relation)
        .values({
          subjectKind: 'person',
          subjectId: me.personId,
          verb: CONTACT,
          resourceKind: 'person',
          resourceId: heldId,
        })
        .onConflictDoNothing();
      people.push(heldId);
    }

    let count = 0;
    for (const personId of new Set(people)) {
      const rows = await tx
        .insert(schema.eventInvite)
        /* A plus-one is offered by default: the first cut counts them and
         * does not name them, and a host who wants to close that door does it
         * per guest in a later rung. */
        .values({ eventId: target, personId, kind: 'personal', plusOneAllowed: true })
        .onConflictDoNothing({
          target: [schema.eventInvite.eventId, schema.eventInvite.personId],
        })
        .returning({ id: schema.eventInvite.id });
      if (rows.length === 0) continue;
      count += 1;
      await writeRelation(tx, { personId, verb: INVITED, eventId: target });
      /* A published event mints the link now; a draft mints them all at
       * publication. */
      if (event.publishedAt !== null) {
        const { linkId } = await mintPersonalLink(tx, { personId, createdBy: me.personId });
        await tx
          .update(schema.eventInvite)
          .set({ linkId })
          .where(eq(schema.eventInvite.id, rows[0]!.id));
      }
    }
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'invite-people',
      targetKind: 'event',
      targetId: target,
      payload: { added: count },
    });
    return count;
  });

  if (added === null) return { error: NO_SUCH };
  revalidatePath(`${EVENTS}/${field(form, 'event')}`);
  return { notice: added === 0 ? 'Everyone there was already invited.' : `${added} invited.` };
}

/** RemoveInvite. The person stays — they are in the host's address book —
 *  and their link stops working. */
export async function removeInvite(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('event', field(form, 'event'));
  const person = tryDecodeId('person', field(form, 'person'));
  if (target === null || person === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    const event = await hostedEvent(tx, me, target);
    if (!event) return false;
    const rows = await tx
      .delete(schema.eventInvite)
      .where(
        and(eq(schema.eventInvite.eventId, target), eq(schema.eventInvite.personId, person)),
      )
      .returning({ linkId: schema.eventInvite.linkId });
    if (rows.length === 0) return false;
    const linkId = rows[0]!.linkId;
    if (linkId !== null) {
      await tx
        .update(schema.link)
        .set({ revokedAt: sql`now()` })
        .where(eq(schema.link.id, linkId));
    }
    await dropRelation(tx, { personId: person, verb: INVITED, eventId: target });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'remove-invite',
      targetKind: 'event',
      targetId: target,
      payload: { person },
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(`${EVENTS}/${field(form, 'event')}`);
  return { notice: 'Removed.' };
}

/** MintLink. A host who lost a URL asks for another one; the old one stops
 *  working, because two live links for one guest is two people. */
export async function mintGuestLink(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('event', field(form, 'event'));
  const person = field(form, 'person') ? tryDecodeId('person', field(form, 'person')) : null;
  if (target === null) return { error: NO_SUCH };

  const minted = await database().transaction(async (tx) => {
    const event = await hostedEvent(tx, me, target);
    if (!event) return null;
    if (person === null) {
      await tx
        .update(schema.link)
        .set({ revokedAt: sql`now()` })
        .where(
          and(
            eq(schema.link.kind, 'event_open'),
            eq(schema.link.targetKind, 'event'),
            eq(schema.link.targetId, target),
            isNull(schema.link.revokedAt),
          ),
        );
      const { token } = await mintOpenLink(tx, { eventId: target, createdBy: me.personId });
      await recordAudit(tx, {
        actorPersonId: me.personId,
        command: 'mint-open-link',
        targetKind: 'event',
        targetId: target,
      });
      return guestUrl(event.slug, token);
    }
    const rows = await tx
      .select({ id: schema.eventInvite.id, linkId: schema.eventInvite.linkId })
      .from(schema.eventInvite)
      .where(and(eq(schema.eventInvite.eventId, target), eq(schema.eventInvite.personId, person)))
      .limit(1);
    const invite = rows[0];
    if (!invite) return null;
    if (invite.linkId !== null) {
      await tx
        .update(schema.link)
        .set({ revokedAt: sql`now()` })
        .where(eq(schema.link.id, invite.linkId));
    }
    const { token, linkId } = await mintPersonalLink(tx, {
      personId: person,
      createdBy: me.personId,
    });
    await tx.update(schema.eventInvite).set({ linkId }).where(eq(schema.eventInvite.id, invite.id));
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'mint-personal-link',
      targetKind: 'event',
      targetId: target,
      payload: { person },
    });
    return guestUrl(event.slug, token);
  });

  if (minted === null) return { error: NO_SUCH };
  revalidatePath(`${EVENTS}/${field(form, 'event')}`);
  return { minted, notice: 'Copy it now — it is not stored and cannot be shown again.' };
}

/** The one action the publish control is bound to. Publishing and
 *  unpublishing are two commands with two audit rows, but a form whose action
 *  changes identity when the page revalidates is a form whose reply is thrown
 *  away — and the reply is where the minted URLs live, once. So the control
 *  keeps one action and names its intent in a field. */
export async function setPublication(prev: FormState, form: FormData): Promise<FormState> {
  return field(form, 'intent') === 'unpublish'
    ? unpublishEvent(prev, form)
    : publishEvent(prev, form);
}
