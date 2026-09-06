/* What one guest may see. The redaction is structural: a field a guest has
 * not earned is absent from this object, not styled out of the page. A page
 * whose source can be read is a page whose secrets have to be missing.
 *
 * Two stages. Before yes: the title, the window, the host, the body, and the
 * who's-coming list as first names, softened. After yes: the address, the
 * details behind it, full names if the host allowed them, and a calendar
 * entry. */

import type { EventRow } from './authority';
import { fullness, headcount, type Fullness, type Headcount } from './capacity';
import { renderBody } from './markup';
import { LIST_SHOWN, firstName } from './ordering';
import { answerOf, answers, guestList, hostName, type ViewerAnswer } from './queries';

export interface GuestName {
  /* First name before an answer; whatever the host allowed after one. */
  label: string;
  shared: boolean;
  plusOne: number;
}

export interface GuestView {
  slug: string;
  title: string;
  startsAt: Date;
  endsAt: Date | null;
  timezone: string;
  location: string | null;
  /* Present only once this viewer has said yes. */
  address: string | null;
  bodyHtml: string;
  colour: string | null;
  posterUrl: string | null;
  hostName: string;
  capacity: number | null;
  room: Fullness;
  counts: Headcount;
  /* Whether the names are still softened. */
  blurred: boolean;
  guests: GuestName[];
  more: number;
  viewer: { name: string; answer: ViewerAnswer | null } | null;
  plusOneAllowed: boolean;
}

export async function guestView(
  event: EventRow,
  viewer: { personId: number | null; name: string | null; plusOneAllowed: boolean } | null,
): Promise<GuestView> {
  const answer = viewer?.personId ? await answerOf(event.id, viewer.personId) : null;
  const said = answer?.response ?? null;
  const sharpened = said === 'yes';
  const rows = await answers(event.id);
  const list = await guestList(event.id, viewer?.personId ?? null, LIST_SHOWN);
  const host = (await hostName(event.hostPersonId)) ?? 'The host';

  return {
    slug: event.slug,
    title: event.title,
    startsAt: event.startsAt,
    endsAt: event.endsAt,
    timezone: event.timezone,
    location: event.location,
    address: sharpened ? event.address : null,
    bodyHtml: renderBody(event.body),
    colour: event.colour,
    posterUrl: event.posterUrl,
    hostName: host,
    capacity: event.capacity,
    room: fullness(event.capacity, rows),
    counts: headcount(rows),
    blurred: !sharpened,
    guests: list.shown.map((guest) => ({
      /* The surname is not in the HTML until the host has allowed it and the
       * viewer has answered yes: a blurred full name is still a full name to
       * anyone who reads the source. */
      label: sharpened && event.revealGuests ? guest.displayName : firstName(guest.displayName),
      shared: guest.shared,
      plusOne: guest.plusOne,
    })),
    more: list.more,
    viewer: viewer ? { name: viewer.name ?? '', answer } : null,
    plusOneAllowed: viewer?.plusOneAllowed ?? false,
  };
}
