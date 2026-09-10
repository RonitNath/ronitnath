/* The calendar entry. A guest who said yes gets the address in it; anyone
 * else with a live link gets the event without it, because a .ics is a file
 * that travels and the address is the thing a yes buys.
 *
 * The UID is stable per event and SEQUENCE is the host's edit count, so a
 * second download replaces the entry a calendar already holds rather than
 * adding another one — that is the whole of "calendars follow the edits". */

import { and, eq } from 'drizzle-orm';
import { NextResponse } from 'next/server';

import { database, schema } from '@/db/client';
import { currentPrincipal } from '@/features/auth/principal';
import { publishedEvent } from '@/features/events/authority';
import { calendarUid, renderCalendar } from '@/features/events/ics';
import { readEventLink } from '@/features/events/links';
import { excerpt } from '@/features/events/markup';
import { publicOrigin } from '@/lib/env';

const DECLINED = new NextResponse('This link does not work.', {
  status: 404,
  headers: { 'content-type': 'text/plain; charset=utf-8', 'cache-control': 'no-store' },
});

export async function GET(
  request: Request,
  { params }: { params: Promise<{ slug: string }> },
) {
  const { slug } = await params;
  const token = new URL(request.url).searchParams.get('l') ?? '';
  const principal = await currentPrincipal();

  const found = await database().transaction(async (tx) => {
    const event = await publishedEvent(tx, slug);
    if (!event) return null;
    let personId: number | null = null;
    if (token) {
      const link = await readEventLink(tx, token);
      if (!link) return null;
      if (link.kind === 'event_personal') personId = link.targetId;
      else if (link.targetId !== event.id) return null;
    } else if (principal) {
      personId = principal.personId;
    } else {
      return null;
    }
    /* The address rides along only for a guest who said yes. */
    let said: string | null = null;
    if (personId !== null) {
      const rows = await tx
        .select({ response: schema.rsvp.response })
        .from(schema.rsvp)
        .where(and(eq(schema.rsvp.eventId, event.id), eq(schema.rsvp.personId, personId)))
        .limit(1);
      said = rows[0]?.response ?? null;
    }
    return { event, coming: said === 'yes' };
  });

  if (!found) return DECLINED;
  const { event, coming } = found;
  const origin = publicOrigin();
  const host = new URL(origin).host;

  const body = renderCalendar({
    uid: calendarUid(event.slug, host),
    sequence: event.sequence,
    title: event.title,
    startsAt: event.startsAt,
    endsAt: event.endsAt,
    location: coming
      ? [event.location, event.address].filter(Boolean).join(', ') || null
      : event.location,
    description: excerpt(event.body, 300) || null,
    url: `${origin}/e/${event.slug}`,
    organizer: null,
    /* The entry's timestamp is the host's last edit, not the clock: the same
     * event downloaded twice is the same bytes. */
    stamp: event.updatedAt,
  });

  return new NextResponse(body, {
    headers: {
      'content-type': 'text/calendar; charset=utf-8',
      'content-disposition': `attachment; filename="${event.slug}.ics"`,
      'cache-control': 'no-store',
    },
  });
}
