/* Every old `/app/...` path, forwarded once and permanently.
 *
 * An optional catch-all so that `/app` itself and `/app/events/e_…` are one
 * rule rather than two. The reply is a 308: the path is gone for good, and a
 * browser, a bookmark and a search engine should all stop asking for it.
 *
 * A reader with no session has nothing to be forwarded to — there is no "me"
 * to put in the path — so they go to the door carrying where they were going,
 * and land on the new path after signing in. That round trip is why this is a
 * page and not a rewrite.
 *
 * The query string is deliberately dropped: `/app` never carried one that
 * outlived a render, and rebuilding it here would mean trusting whatever a
 * stale link brought with it. */

import { permanentRedirect, redirect } from 'next/navigation';

import { currentPrincipal } from '@/features/auth/principal';
import { encodeId } from '@/lib/ids';
import { LEGACY_APP_ROOT, userPath } from '@/lib/paths';
import { signInPath } from '@/lib/tiers';

export const dynamic = 'force-dynamic';

export default async function LegacyAppPath({
  params,
}: {
  params: Promise<{ rest?: string[] }>;
}) {
  const { rest } = await params;
  const view = (rest ?? []).map((segment) => encodeURIComponent(segment)).join('/');

  const principal = await currentPrincipal();
  if (principal === null) {
    redirect(signInPath(view === '' ? LEGACY_APP_ROOT : `${LEGACY_APP_ROOT}/${view}`));
  }

  permanentRedirect(userPath(encodeId('person', principal.personId), view));
}
