/* The calendar entry. RFC 5545, written by hand: one VEVENT, folded at 75
 * octets, CRLF line endings.
 *
 * Two properties carry the whole promise that an edit follows the guest.
 * UID is derived from the event and never changes, so a second download is
 * the same entry rather than a second one; SEQUENCE is the host's edit count,
 * and a calendar that already holds this UID replaces what it has when the
 * sequence it is handed is higher. Nothing here reads the clock — the caller
 * passes DTSTAMP — so the same event renders byte for byte the same twice. */

export interface CalendarEvent {
  uid: string;
  sequence: number;
  title: string;
  startsAt: Date;
  endsAt: Date | null;
  /* What a guest may see: the place name always, the address only once the
   * caller has decided this guest has earned it. */
  location: string | null;
  description: string | null;
  url: string | null;
  organizer: string | null;
  stamp: Date;
  /* An unpublished or cancelled event still answers, so a calendar that
   * already holds the entry is told it is gone rather than left with it. */
  cancelled?: boolean;
}

/* An event without an end is an hour and a half: a calendar has to draw
 * something, and a zero-length block reads as a reminder, not an evening. */
export const DEFAULT_MINUTES = 90;

function stamp(at: Date): string {
  return `${at.toISOString().replace(/[-:]/g, '').slice(0, 15)}Z`;
}

/* RFC 5545 §3.3.11: backslash, semicolon and comma are escaped; a newline
 * becomes a literal \n. */
function escapeText(value: string): string {
  return value
    .replace(/\\/g, '\\\\')
    .replace(/;/g, '\\;')
    .replace(/,/g, '\\,')
    .replace(/\r\n?|\n/g, '\\n');
}

/* §3.1: no line longer than 75 octets, continued by a space. Folding counts
 * bytes, not characters, so a multi-byte character is never split. */
function fold(line: string): string[] {
  const bytes = Buffer.from(line, 'utf8');
  if (bytes.length <= 75) return [line];
  const out: string[] = [];
  let start = 0;
  let limit = 75;
  while (start < bytes.length) {
    let end = Math.min(start + limit, bytes.length);
    /* Back off a continuation byte so a code point stays whole. */
    while (end > start && end < bytes.length && (bytes[end]! & 0xc0) === 0x80) end -= 1;
    out.push((out.length === 0 ? '' : ' ') + bytes.subarray(start, end).toString('utf8'));
    start = end;
    limit = 74;
  }
  return out;
}

export function renderCalendar(input: CalendarEvent): string {
  const end =
    input.endsAt ?? new Date(input.startsAt.getTime() + DEFAULT_MINUTES * 60_000);
  const lines: string[] = [
    'BEGIN:VCALENDAR',
    'VERSION:2.0',
    'PRODID:-//ronitnath.com//events//EN',
    'CALSCALE:GREGORIAN',
    `METHOD:${input.cancelled ? 'CANCEL' : 'PUBLISH'}`,
    'BEGIN:VEVENT',
    `UID:${input.uid}`,
    `SEQUENCE:${input.sequence}`,
    `DTSTAMP:${stamp(input.stamp)}`,
    `DTSTART:${stamp(input.startsAt)}`,
    `DTEND:${stamp(end)}`,
    `SUMMARY:${escapeText(input.title)}`,
    `STATUS:${input.cancelled ? 'CANCELLED' : 'CONFIRMED'}`,
    'TRANSP:OPAQUE',
  ];
  if (input.location) lines.push(`LOCATION:${escapeText(input.location)}`);
  if (input.description) lines.push(`DESCRIPTION:${escapeText(input.description)}`);
  if (input.url) lines.push(`URL:${escapeText(input.url)}`);
  if (input.organizer) lines.push(`ORGANIZER;CN=${escapeText(input.organizer)}:invalid:nomail`);
  lines.push('END:VEVENT', 'END:VCALENDAR');
  return lines.flatMap(fold).join('\r\n') + '\r\n';
}

/** Stable for the life of the event, and unique to this deployment: the slug
 *  is what a guest already has and what a second download must agree with. */
export function calendarUid(slug: string, host: string): string {
  return `${slug}@${host}`;
}
