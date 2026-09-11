/* One person's event stream.
 *
 * WHY IT IS AT `…/events/stream` AND NOT `…/events`
 *
 * The fleet contract puts the stream at `/o/{org}/events` and, for this site,
 * `/u/{user}/events` (payload-removal/contracts.md §SSE). On ronitnath that
 * second path is already taken: `/u/{user}/events` is the page listing the
 * events somebody is hosting, and a Next route segment is either a page or a
 * route handler, never both — putting `route.ts` beside that `page.tsx` is a
 * build error, and moving the page would break a surface people use and link
 * to in order to make room for one a machine reads.
 *
 * So the stream took the name that cannot collide. `stream` is a static
 * segment and wins over the sibling `[id]`, and no event's public id can ever
 * be the literal string `stream` — they are `e_`-prefixed — so the guest list
 * at `/u/{user}/events/e_…` is untouched.
 *
 * THIS CHANGES THE EDGE. The Caddy fragment's SSE matcher for this app is
 * `/u/*​/events/stream` rather than `/u/*​/events`; the org stream below is
 * unmoved at `/o/*​/events`. Flagged to Yulier with the leg report — the two
 * matchers are exact paths, which is better than the prefix match a page and
 * a stream sharing one name would have forced.
 *
 * WHO MAY OPEN IT
 *
 * The subject, or a platform operator reading them: the same question
 * `requireSubjectPerson` asks for every page under `/u/{user}`, asked the same
 * way, so the stream cannot be a second door into anything the pages refuse.
 * Everything that is not a yes is a 404 with no body — including an anonymous
 * reader, who on a page would be redirected to the door. A redirect is the
 * wrong answer to an `EventSource`: it would follow it, be handed the sign-in
 * HTML, fail to parse it and reconnect every three seconds for as long as the
 * tab is open.
 *
 * WHAT REACHES IT
 *
 * `org_id` on this site is a public id, and for this stream it is the
 * subject's own. `eventsSince` filters on it, so the partition is the reach
 * boundary: every row here is about a resource on this person's own surfaces,
 * which is precisely the set the gate above just granted. `reach` re-asserts
 * that per row rather than trusting the query, because it costs nothing and
 * the alternative is a predicate that says `true` and stops being read.
 *
 * The one thing this cannot do is notice that authority changed mid-stream —
 * `reach` is synchronous and a database round trip per row per connection is
 * not a thing to put behind a fan-out. It does not have to: a frame carries
 * `{resource_kind, resource_id, seq, published}` and no data at all, and the
 * refetch it provokes goes through the page, which asks the gate again. The
 * worst a stale connection can do is cause a refresh that answers 404. */

import { database } from '@/db/client';
import { currentPrincipal } from '@/features/auth/principal';
import { streamResponse } from '@/lib/fleet/sse';
import { tryDecodeId } from '@/lib/ids';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

/** The uniform decline. No body, no hint, and the same answer for a stranger,
 *  a member reading somebody else's stream and a person who is not there. */
function declined(): Response {
  return new Response(null, { status: 404 });
}

export async function GET(
  request: Request,
  { params }: { params: Promise<{ user: string }> },
): Promise<Response> {
  const { user } = await params;

  const subjectPersonId = tryDecodeId('person', user);
  if (subjectPersonId === null) return declined();

  const principal = await currentPrincipal();
  if (principal === null) return declined();
  if (subjectPersonId !== principal.personId && !principal.isOperator) return declined();

  /* `Last-Event-ID` is the browser's own resumption header and the hook's
   * `?since=` is ours; either is a sequence this connection has already been
   * told about, and the larger of the two is where to carry on from. */
  const header = Number(request.headers.get('last-event-id'));
  const query = Number(new URL(request.url).searchParams.get('since'));
  const since = Math.max(Number.isFinite(header) ? header : 0, Number.isFinite(query) ? query : 0);

  return streamResponse({
    db: database(),
    orgId: user,
    since,
    reach: (row) => row.orgId === user,
  });
}
