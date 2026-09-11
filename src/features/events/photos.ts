/* When an event is taking pictures, and from whom.
 *
 * The window a picture may arrive in is deliberately not the window an answer
 * may. An answer is about an evening that has not happened yet, so the page
 * takes one the moment it is published; a picture is about one that has, so
 * nothing opens until the event has started. It then stays open for a
 * fortnight past the end, which is roughly how long it takes everybody to
 * remember they still have the photographs on their phone.
 *
 * Only a guest who said yes may add one. Maybe is not a yes and a note is not
 * a yes: the gallery of an evening is the people who were at it.
 *
 * Pure, like `capacity.ts` beside it — a rule that can be read off a row and a
 * clock is a rule that can be proved without a database.
 */

import type { EventRow } from './authority';

/** Fifteen megabytes. A phone's photograph is two to five; this is room, not
 *  licence. */
export const MAX_BYTES = 15 * 1024 * 1024;

/** How long past the end of an event pictures are still taken. */
export const GRACE_MS = 14 * 24 * 60 * 60 * 1000;

/** How many one guest may add in an hour. An evening is not a live stream,
 *  and the limit is counted off the pictures themselves rather than a second
 *  table: the rows are the record of what was sent. */
export const PHOTO_RULE = { max: 60, windowMs: 60 * 60 * 1000 };

type Window = Pick<EventRow, 'publishedAt' | 'startsAt' | 'endsAt'>;

/** Whether this event is taking photographs now. An unpublished event never
 *  is: it has no guests yet, only a draft of a page. */
export function photosOpen(event: Window, now = Date.now()): boolean {
  if (event.publishedAt === null) return false;
  if (event.startsAt.getTime() > now) return false;
  const ended = (event.endsAt ?? event.startsAt).getTime();
  return now <= ended + GRACE_MS;
}

/** Whether this guest may add one. */
export function mayAddPhoto(
  event: Window,
  answer: { response: 'yes' | 'maybe' | 'no' } | null,
  now = Date.now(),
): boolean {
  return photosOpen(event, now) && answer?.response === 'yes';
}
