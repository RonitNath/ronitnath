/* The bytes of one picture.
 *
 * There is no bucket and no CDN in this deployment, so the picture is a column
 * and this route is the only way out of it. That is the point rather than a
 * compromise: an object store hands out a URL that answers to anybody who has
 * it for as long as the object exists, and a gallery of somebody's living room
 * is exactly the thing that should stop answering the moment the host takes it
 * down. Here the gate is the event page's own gate — a live link or a session
 * that was already allowed to read the page — and a hidden picture is a 404,
 * including for the browser that has its URL in history.
 *
 * The link travels in the query, as it does for the calendar entry: an `<img>`
 * carries the page's cookies but not the page's address, so a guest reading
 * through a personal link has to hand the same link to the picture.
 *
 * `private` caching, five minutes: long enough that scrolling a gallery does
 * not refetch several megabytes per picture, short enough that taking one down
 * is over within one. `private` because the response is authorised and must
 * never be held by anything between here and the guest.
 */

import { and, eq } from 'drizzle-orm';
import { NextResponse } from 'next/server';

import { database, schema } from '@/db/client';
import { currentPrincipal } from '@/features/auth/principal';
import { publishedEvent } from '@/features/events/authority';
import { readEventLink } from '@/features/events/links';
import { photoBytes } from '@/features/events/queries';
import { tryDecodeId } from '@/lib/ids';

export const dynamic = 'force-dynamic';

const MISSING = new NextResponse('Not here.', {
  status: 404,
  headers: { 'content-type': 'text/plain; charset=utf-8', 'cache-control': 'no-store' },
});

export async function GET(
  request: Request,
  { params }: { params: Promise<{ slug: string; id: string }> },
) {
  const { slug, id } = await params;
  const photoId = tryDecodeId('photo', id);
  if (photoId === null) return MISSING;
  const token = new URL(request.url).searchParams.get('l') ?? '';
  const principal = await currentPrincipal();

  const allowed = await database().transaction(async (tx) => {
    const event = await publishedEvent(tx, slug);
    if (!event) return null;
    if (token) {
      const link = await readEventLink(tx, token);
      if (!link) return null;
      if (link.kind === 'event_open' && link.targetId !== event.id) return null;
      if (link.kind === 'event_personal') {
        const invited = await tx
          .select({ id: schema.eventInvite.id })
          .from(schema.eventInvite)
          .where(
            and(
              eq(schema.eventInvite.eventId, event.id),
              eq(schema.eventInvite.personId, link.targetId),
            ),
          )
          .limit(1);
        if (invited.length === 0) return null;
      }
      return { eventId: event.id, hidden: false };
    }
    if (!principal) return null;
    if (principal.personId === event.hostPersonId || principal.isOperator) {
      return { eventId: event.id, hidden: true };
    }
    const invited = await tx
      .select({ id: schema.eventInvite.id })
      .from(schema.eventInvite)
      .where(
        and(
          eq(schema.eventInvite.eventId, event.id),
          eq(schema.eventInvite.personId, principal.personId),
        ),
      )
      .limit(1);
    return invited.length === 0 ? null : { eventId: event.id, hidden: false };
  });
  if (allowed === null) return MISSING;

  const picture = await photoBytes(allowed.eventId, photoId, { hidden: allowed.hidden });
  if (!picture) return MISSING;

  return new NextResponse(new Uint8Array(picture.bytes), {
    headers: {
      'content-type': picture.mimeType,
      'content-length': String(picture.bytes.length),
      'cache-control': 'private, max-age=300',
    },
  });
}
