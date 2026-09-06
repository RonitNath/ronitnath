/* Who's coming, in the order the viewer should read it.
 *
 * The ruling is that people the viewer shares a circle with come first — and
 * circles are groups, which arrive in R5. So the ordering asks one question,
 * `sharedWith`, and takes whatever the caller can answer today: R4 answers it
 * from `relation` rows (two people a member has written down as contacts are
 * as close to a shared circle as this rung has), and R5 replaces the query
 * behind it without touching this file or the page that calls it.
 *
 * Everything after that is answered-order, oldest first: the list is social
 * proof, and the person who answered first has been coming the longest. */

export interface Guest {
  personId: number;
  displayName: string;
  answeredAt: Date;
  plusOne: number;
}

export interface Ordered extends Guest {
  /* Why this row is where it is. The page reads it to decide nothing — it is
   * here so a test can see the hook worked. */
  shared: boolean;
}

export function orderGuests(
  guests: readonly Guest[],
  /* The person ids the viewer shares a circle with. Empty is the honest
   * answer for a viewer who shares none, and for a deployment where circles
   * do not exist yet. */
  sharedWith: ReadonlySet<number>,
): Ordered[] {
  return guests
    .map((guest) => ({ ...guest, shared: sharedWith.has(guest.personId) }))
    .sort((a, b) => {
      if (a.shared !== b.shared) return a.shared ? -1 : 1;
      const when = a.answeredAt.getTime() - b.answeredAt.getTime();
      return when !== 0 ? when : a.personId - b.personId;
    });
}

/** First name only, which is what a guest reads before they have answered.
 *  The blur is CSS; this is the part that is not merely visual, because a
 *  softened surname is still a surname in the HTML. */
export function firstName(displayName: string): string {
  return displayName.trim().split(/\s+/)[0] ?? displayName;
}

/** How many names to show before "and N more". */
export const LIST_SHOWN = 8;
