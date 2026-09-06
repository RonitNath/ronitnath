/* The slug is the event's public name. It is derived from the title once, at
 * creation, and then it belongs to the URL: a host who rewrites the title
 * does not break links that are already pasted into other people's chats.
 *
 * Uniqueness is settled by the database, not by a lookup — the caller retries
 * with the next candidate when the unique index says no — so two hosts naming
 * the same evening at the same moment cannot both win. */

const MAX = 48;

/** The bare stem: lower case, words joined by hyphens, nothing that has to be
 *  percent-encoded, never empty and never longer than a line of a URL. */
export function slugify(title: string): string {
  const stem = title
    .normalize('NFKD')
    /* Combining marks, once NFKD has separated them from their letters. */
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, MAX)
    .replace(/-+$/, '');
  return stem.length > 0 ? stem : 'event';
}

/** Candidate `n` for a title: the stem, then the stem with a suffix. The
 *  first candidate is the pretty one, so the common case has no suffix at
 *  all, and the sequence never ends. */
export function slugCandidate(title: string, attempt: number): string {
  const stem = slugify(title);
  if (attempt === 0) return stem;
  /* Past a handful of collisions the count stops being informative and the
   * point is only to land somewhere free: four base32 characters. */
  const tail =
    attempt < 10 ? String(attempt + 1) : Math.random().toString(32).slice(2, 6).padEnd(4, '0');
  return `${stem.slice(0, MAX - tail.length - 1)}-${tail}`;
}
