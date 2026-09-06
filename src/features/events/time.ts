/* Time, in somebody's zone.
 *
 * An instant is stored as an instant. What a host types and what a guest
 * reads are wall clocks in a named zone, and the only correct way to cross
 * between them in a browser and on a server alike is Intl — the zone's rules
 * for that date, not an offset somebody wrote down once. */

const PARTS: Intl.DateTimeFormatOptions = {
  year: 'numeric',
  month: '2-digit',
  day: '2-digit',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  hour12: false,
};

function fields(at: Date, timeZone: string): Record<string, number> {
  const parts = new Intl.DateTimeFormat('en-US', { ...PARTS, timeZone }).formatToParts(at);
  const out: Record<string, number> = {};
  for (const part of parts) if (part.type !== 'literal') out[part.type] = Number(part.value);
  /* Midnight comes back as hour 24 from some ICU versions. */
  if (out.hour === 24) out.hour = 0;
  return out;
}

/** The offset of a zone at an instant, in minutes east of UTC. */
export function offsetMinutes(at: Date, timeZone: string): number {
  const f = fields(at, timeZone);
  const asUtc = Date.UTC(f.year!, f.month! - 1, f.day!, f.hour!, f.minute!, f.second!);
  return Math.round((asUtc - at.getTime()) / 60_000);
}

/** A wall clock in a zone → the instant it names. Two passes, because the
 *  offset itself depends on the instant: the first guess is corrected by the
 *  offset that actually applies there. */
export function fromWallClock(local: string, timeZone: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2})/.exec(local.trim());
  if (!m) return null;
  const [, y, mo, d, h, mi] = m.map(Number) as unknown as number[];
  const naive = Date.UTC(y!, mo! - 1, d!, h!, mi!);
  const first = new Date(naive - offsetMinutes(new Date(naive), timeZone) * 60_000);
  const second = new Date(naive - offsetMinutes(first, timeZone) * 60_000);
  return Number.isNaN(second.getTime()) ? null : second;
}

/** An instant → the value a `datetime-local` input wants, in a zone. */
export function toWallClock(at: Date, timeZone: string): string {
  const f = fields(at, timeZone);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${f.year}-${pad(f.month!)}-${pad(f.day!)}T${pad(f.hour!)}:${pad(f.minute!)}`;
}

export function isZone(name: string): boolean {
  try {
    new Intl.DateTimeFormat('en-US', { timeZone: name });
    return true;
  } catch {
    return false;
  }
}

/** How a guest reads the time: the day, and the window if there is an end. */
export function readableWindow(
  startsAt: Date,
  endsAt: Date | null,
  timeZone: string,
  locale = 'en-US',
): string {
  const day = new Intl.DateTimeFormat(locale, {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
    timeZone,
  }).format(startsAt);
  const clock = new Intl.DateTimeFormat(locale, {
    hour: 'numeric',
    minute: '2-digit',
    timeZone,
  });
  const start = clock.format(startsAt);
  if (!endsAt) return `${day}, ${start}`;
  const sameDay = toWallClock(startsAt, timeZone).slice(0, 10) === toWallClock(endsAt, timeZone).slice(0, 10);
  return sameDay
    ? `${day}, ${start} – ${clock.format(endsAt)}`
    : `${day}, ${start} – ${new Intl.DateTimeFormat(locale, { month: 'long', day: 'numeric', hour: 'numeric', minute: '2-digit', timeZone }).format(endsAt)}`;
}

/** The zone's short name, so a guest reading in another one knows which
 *  clock the page is speaking. */
export function zoneLabel(at: Date, timeZone: string, locale = 'en-US'): string {
  const parts = new Intl.DateTimeFormat(locale, { timeZone, timeZoneName: 'short' }).formatToParts(at);
  return parts.find((part) => part.type === 'timeZoneName')?.value ?? timeZone;
}
