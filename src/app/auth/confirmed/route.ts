/* Where a re-authentication round trip through ZITADEL comes back to.
 *
 * An operator who signs in through ZITADEL has no password on this side, so
 * the only proof they are still at the keyboard is a fresh round trip: the OP
 * is asked to authenticate again and better-auth mints a *new* session when
 * the callback lands. That is what this route reads. A session created seconds
 * ago is a round trip that just happened, and stamping
 * `session.reauthenticated_at` is the record of it.
 *
 * The window is deliberately tight. It is not an authorization check — the
 * session already is the operator's — it is the difference between "you came
 * back through the OP just now" and "you have been signed in since Tuesday",
 * and only the first one opens the ten-minute window.
 *
 * A route handler rather than a page because it writes: a Server Component
 * that stamps a column on every render is a read that is not a read. */

import { eq, sql } from 'drizzle-orm';
import { NextResponse, type NextRequest } from 'next/server';

import { authSchema, database } from '@/db/client';
import { auth } from '@/features/auth/auth';
import { publicOrigin } from '@/lib/env';
import { OPERATOR_ROOT, operatorPath } from '@/lib/paths';

export const dynamic = 'force-dynamic';

const FRESH_SECONDS = 120;

/** Only a path back into the operator surface. */
function safeNext(raw: string | null): string {
  return raw && new RegExp(`^${OPERATOR_ROOT}(/[A-Za-z0-9\\-._~/]*)?$`).test(raw)
    ? raw
    : operatorPath();
}

export async function GET(request: NextRequest): Promise<NextResponse> {
  const next = safeNext(request.nextUrl.searchParams.get('next'));
  const found = await auth().api.getSession({ headers: request.headers });

  if (found) {
    const age = Date.now() - new Date(found.session.createdAt).getTime();
    if (age >= 0 && age <= FRESH_SECONDS * 1000) {
      await database()
        .update(authSchema.session)
        .set({ reauthenticatedAt: sql`now()` })
        .where(eq(authSchema.session.id, found.session.id));
    }
  }

  return NextResponse.redirect(new URL(next, publicOrigin()));
}
