/* One organization's event stream, at the path the fleet contract names
 * (payload-removal/contracts.md §SSE). Nothing collides here: `/o/{org}` is a
 * single page, so `events` was free.
 *
 * Who may open it is the question `/o/{org}` itself asks — admin or owner of
 * the organization, or a platform operator — and it is asked with the same
 * gate. That matters more here than on a person's stream: this site's `/o`
 * page is an admin surface, and a plain member who could open the stream
 * would be told that documents and invitations they cannot read had changed.
 * `requireOrgOperator` is the predicate the page uses, so it is the predicate
 * the stream uses.
 *
 * Everything else — the 404 in place of a redirect, `org_id` as the reach
 * boundary, and why a synchronous `reach` is enough when a frame carries no
 * data — is set out at length in the sibling `/u/[user]/events/stream`. */

import { database } from '@/db/client';
import { currentPrincipal } from '@/features/auth/principal';
import { operatedOrganization } from '@/features/organizations/queries';
import { streamResponse } from '@/lib/fleet/sse';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

function declined(): Response {
  return new Response(null, { status: 404 });
}

export async function GET(
  request: Request,
  { params }: { params: Promise<{ org: string }> },
): Promise<Response> {
  const { org } = await params;

  const principal = await currentPrincipal();
  if (principal === null) return declined();

  const organization = await operatedOrganization(org, {
    personId: principal.personId,
    isOperator: principal.isOperator,
  });
  if (organization === null) return declined();

  const header = Number(request.headers.get('last-event-id'));
  const query = Number(new URL(request.url).searchParams.get('since'));
  const since = Math.max(Number.isFinite(header) ? header : 0, Number.isFinite(query) ? query : 0);

  /* The handle, not the row id: `org_id` is what appears in the URL, and a
   * renamed organization is a new address for everyone including its own
   * stream. */
  return streamResponse({
    db: database(),
    orgId: organization.handle,
    since,
    reach: (row) => row.orgId === organization.handle,
  });
}
