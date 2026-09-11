'use server';

/* The two commands about pictures: a guest adds one, the host takes one down.
 *
 * They are not in `actions.ts` and not in `respond.ts`, for the reason those
 * two are already apart. The host's commands there are authorised by a session
 * and the guest's by a bearer link, and this file has one of each — but what
 * actually separates it is that a file is not a field. Everything between the
 * browser and the row here is about bytes: how many there may be, what they
 * may claim to be, and what has to come out of them before they are stored
 * anywhere (`image.ts`).
 *
 * The gate is also a different gate. An answer wants an event that is
 * published; a picture wants one that has *started*, and it stays open for a
 * fortnight after it ends (`photos.ts`).
 *
 * The refusals are in the register the rest of the guest page uses: one
 * sentence, saying what happened and nothing about what else exists. The one
 * exception is a HEIC, which is named, because "that kind of picture" is a
 * thing a guest can act on — their phone can send a JPEG instead.
 */

import { and, eq, gte, isNull, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { Transaction } from '@/features/auth/db';
import { currentPrincipal } from '@/features/auth/principal';
import type { FormState } from '@/features/auth/form-state';
import { emit } from '@/lib/fleet/events';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { userPath } from '@/lib/paths';
import { hostedEvent, publishedEvent } from './authority';
import { dimensionsOf, isAllowed, isHeic, sniff, stripMetadata } from './image';
import { readEventLink } from './links';
import { MAX_BYTES, PHOTO_RULE, mayAddPhoto, photosOpen } from './photos';

/* One sentence for every closed door, as the rest of the feature says. */
const SHUT = 'This page is not taking pictures.';
const NOT_YOURS = 'Pictures are for guests who said yes.';
const NO_SUCH = 'That is not something you can do here.';

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

/** Who is adding this, established exactly as the guest page established who
 *  is reading: a personal link names a person, and a session names itself. An
 *  open link names only the event, and a guest who answered through one is
 *  holding a personal link of their own by the time there is anything to
 *  photograph. */
async function uploader(
  tx: Transaction,
  input: { eventId: number; token: string; sessionPersonId: number | null },
): Promise<number | null> {
  if (input.token) {
    const link = await readEventLink(tx, input.token);
    if (!link || link.kind !== 'event_personal') return null;
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
    return invited.length === 0 ? null : link.targetId;
  }
  return input.sessionPersonId;
}

/** AddPhoto. The picture, or a sentence saying why not.
 *
 *  The order of the work is the order of what it costs. The event, the guest
 *  and their answer are three indexed reads; walking several megabytes of
 *  JPEG is the expensive part and happens once everything cheap has agreed
 *  that it should. The write then re-reads the gate inside its own
 *  transaction, because the cheap reads were a different one. */
export async function addPhoto(_prev: FormState, form: FormData): Promise<FormState> {
  const slug = field(form, 'slug');
  const token = field(form, 'token');
  if (!slug) return { error: SHUT };
  const principal = await currentPrincipal();

  const gate = await database().transaction(async (tx) => {
    const event = await publishedEvent(tx, slug);
    if (!event || !photosOpen(event)) return 'shut' as const;
    const personId = await uploader(tx, {
      eventId: event.id,
      token,
      sessionPersonId: principal?.personId ?? null,
    });
    if (personId === null) return 'not-yours' as const;
    const answer = await tx
      .select({ response: schema.rsvp.response })
      .from(schema.rsvp)
      .where(and(eq(schema.rsvp.eventId, event.id), eq(schema.rsvp.personId, personId)))
      .limit(1);
    if (!mayAddPhoto(event, answer[0] ?? null)) return 'not-yours' as const;
    return { eventId: event.id, hostPersonId: event.hostPersonId, personId };
  });
  if (gate === 'shut') return { error: SHUT };
  if (gate === 'not-yours') return { error: NOT_YOURS };

  /* Duck-typed rather than `instanceof File`. What arrives here is whichever
   * `File` the running realm built the FormData with — undici's under Next,
   * the platform's in a test — and a nominal check against whatever `File`
   * this module happens to see refuses a perfectly good file for being the
   * wrong brand of one. What is needed of it is that it has bytes and says
   * what they are, so that is what is asked. */
  const file = form.get('photo') as Partial<File> | null;
  if (!file || typeof file.arrayBuffer !== 'function' || !file.size || !file.type) {
    return { error: 'Choose a picture first.' };
  }
  if (isHeic(file.type)) {
    return { error: 'That is a HEIC. Send a JPEG and it will go straight up.' };
  }
  if (!isAllowed(file.type)) {
    return { error: 'Pictures only — a JPEG, a PNG or a WebP.' };
  }
  if (file.size > MAX_BYTES) {
    return { error: 'That picture is over 15 MB. A smaller copy will do.' };
  }

  const raw = new Uint8Array(await file.arrayBuffer());
  /* What the bytes are, not what the browser said they are: a file that calls
   * itself a JPEG and is not is refused here rather than stored and served as
   * one. */
  const mime = sniff(raw);
  if (!mime) return { error: 'That file could not be read as a picture.' };
  const clean = stripMetadata(raw, mime);
  const size = dimensionsOf(clean, mime);
  if (!size) return { error: 'That file could not be read as a picture.' };

  const stored = await database().transaction(async (tx) => {
    const event = await publishedEvent(tx, slug);
    if (!event || event.id !== gate.eventId || !photosOpen(event)) return null;

    /* The limit is counted off the pictures themselves. A separate counter
     * table would be a second thing to keep in step with the rows it is
     * counting, and the rows are already the record of what was sent. */
    const since = new Date(Date.now() - PHOTO_RULE.windowMs);
    const recent = await tx
      .select({ n: sql<number>`count(*)::int` })
      .from(schema.photo)
      .where(
        and(
          eq(schema.photo.personId, gate.personId),
          gte(schema.photo.createdAt, since),
        ),
      );
    if ((recent[0]?.n ?? 0) >= PHOTO_RULE.max) return 'too-many' as const;

    const rows = await tx
      .insert(schema.photo)
      .values({
        eventId: event.id,
        personId: gate.personId,
        mimeType: mime,
        bytes: Buffer.from(clean),
        byteSize: clean.length,
        width: size.width,
        height: size.height,
      })
      .returning({ id: schema.photo.id });
    const row = rows[0];
    if (!row) return null;

    await recordAudit(tx, {
      actorPersonId: gate.personId,
      command: 'add-photo',
      targetKind: 'photo',
      targetId: row.id,
      payload: { event: encodeId('event', event.id), bytes: clean.length, mimeType: mime },
    });
    /* On the host's stream, like an rsvp and for the same reason: a picture is
     * a change to a page the host is watching, and a guest has no stream of
     * their own. */
    await emit(tx, {
      orgId: encodeId('person', event.hostPersonId),
      resourceKind: 'photo',
      resourceId: encodeId('event', event.id),
      kind: 'added',
    });
    return row.id;
  });

  if (stored === 'too-many') {
    return { error: 'That is a lot of pictures at once. Try again a little later.' };
  }
  if (stored === null) return { error: SHUT };
  revalidatePath(`/e/${slug}`);
  revalidatePath(
    userPath(encodeId('person', gate.hostPersonId), `events/${encodeId('event', gate.eventId)}`),
  );
  return { notice: 'Added.' };
}

/** HidePhoto. The host takes one off the gallery, or puts it back.
 *
 *  `hiddenAt` rather than a delete: it is the host's everyday act, it has to
 *  be reversible, and the row is also the only thing that says the picture was
 *  ever there. */
export async function setPhotoHidden(_prev: FormState, form: FormData): Promise<FormState> {
  const principal = await currentPrincipal();
  if (!principal) return { error: NO_SUCH };
  const eventId = tryDecodeId('event', field(form, 'event'));
  const photoId = tryDecodeId('photo', field(form, 'photo'));
  if (eventId === null || photoId === null) return { error: NO_SUCH };
  const hide = field(form, 'hidden') === '1';

  const done = await database().transaction(async (tx) => {
    const event = await hostedEvent(tx, principal, eventId);
    if (!event) return null;
    const rows = await tx
      .update(schema.photo)
      .set({
        hiddenAt: hide ? sql`now()` : null,
        hiddenBy: hide ? principal.personId : null,
      })
      .where(
        and(
          eq(schema.photo.id, photoId),
          eq(schema.photo.eventId, eventId),
          hide ? isNull(schema.photo.hiddenAt) : undefined,
        ),
      )
      .returning({ id: schema.photo.id });
    if (!rows[0]) return null;

    await recordAudit(tx, {
      actorPersonId: principal.personId,
      command: hide ? 'hide-photo' : 'show-photo',
      targetKind: 'photo',
      targetId: photoId,
      payload: { event: encodeId('event', eventId) },
    });
    await emit(tx, {
      orgId: encodeId('person', event.hostPersonId),
      resourceKind: 'photo',
      resourceId: encodeId('event', eventId),
      kind: hide ? 'hidden' : 'shown',
    });
    return { slug: event.slug, host: event.hostPersonId };
  });

  if (done === null) return { error: NO_SUCH };
  revalidatePath(`/e/${done.slug}`);
  revalidatePath(userPath(encodeId('person', done.host), `events/${field(form, 'event')}`));
  return { notice: hide ? 'Hidden.' : 'Back on the page.' };
}
