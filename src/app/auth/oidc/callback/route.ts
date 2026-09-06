/* The ZITADEL callback. One transaction: find or make the person behind the
 * subject, keep the operator relation true, mint a session, write the audit
 * row. A subject that is not the allowlisted one writes nothing and is told
 * only that it did not work. */

import { and, eq, sql } from 'drizzle-orm';
import { NextResponse } from 'next/server';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import { OIDC_COOKIE, acceptCallback, type OidcChecks } from '@/features/auth/oidc';
import {
  createPerson,
  findIdentity,
  grantOperator,
  seedSubject,
} from '@/features/auth/provision';
import {
  createSession,
  requestFingerprint,
  sessionCookieOptions,
} from '@/features/auth/session';
import { isProduction, sessionCookieName } from '@/lib/env';

export const dynamic = 'force-dynamic';

function declined(request: Request) {
  const response = NextResponse.redirect(new URL('/auth?declined=1', request.url), 302);
  response.cookies.set(OIDC_COOKIE, '', { path: '/auth/oidc', maxAge: 0 });
  return response;
}

function readChecks(request: Request): OidcChecks | null {
  const raw = request.headers
    .get('cookie')
    ?.split('; ')
    .find((pair) => pair.startsWith(`${OIDC_COOKIE}=`))
    ?.slice(OIDC_COOKIE.length + 1);
  if (!raw) return null;
  try {
    const parsed: unknown = JSON.parse(decodeURIComponent(raw));
    if (
      typeof parsed === 'object' &&
      parsed !== null &&
      typeof (parsed as OidcChecks).state === 'string' &&
      typeof (parsed as OidcChecks).nonce === 'string' &&
      typeof (parsed as OidcChecks).codeVerifier === 'string'
    ) {
      return parsed as OidcChecks;
    }
  } catch {
    /* a cookie somebody edited is a cookie we do not have */
  }
  return null;
}

export async function GET(request: Request) {
  const checks = readChecks(request);
  if (!checks) return declined(request);

  const accepted = await acceptCallback(new URL(request.url), checks);
  if (!accepted) return declined(request);

  const where = await requestFingerprint();
  const token = await database().transaction(async (tx) => {
    /* Linked by `sub`, not by address: an address can be renamed at the OP and
     * the subject cannot. Three cases, in order — the subject we already know;
     * the placeholder `pnpm seed:operator` left behind, which this sign-in
     * turns into the real subject so the seeded person is the one that signs
     * in; and nobody, which is a first sign-in with no seed. */
    let identity = await findIdentity(tx, 'oidc', accepted.subject);
    if (!identity) {
      const seeded = await findIdentity(tx, 'oidc', seedSubject(accepted.email));
      if (seeded) {
        await tx
          .update(schema.identity)
          .set({ subject: accepted.subject })
          .where(eq(schema.identity.id, seeded.id));
        identity = seeded;
      }
    }
    let personId: number;
    if (identity) {
      personId = identity.personId;
      await tx
        .update(schema.identity)
        .set({ verifiedAt: sql`now()` })
        .where(eq(schema.identity.id, identity.id));
    } else {
      personId = await createPerson(tx, { displayName: accepted.displayName });
      const rows = await tx
        .insert(schema.identity)
        .values({
          personId,
          source: 'oidc',
          subject: accepted.subject,
          verifiedAt: sql`now()`,
        })
        .returning({ id: schema.identity.id });
      identity = { id: rows[0]!.id, personId, verifiedAt: new Date() };
    }
    await tx
      .insert(schema.factor)
      .values({
        identityId: identity.id,
        kind: 'oidc',
        meta: { issuer: process.env.OIDC_ISSUER, email: accepted.email },
      })
      .onConflictDoNothing();

    await grantOperator(tx, personId);
    const minted = await createSession(tx, {
      personId,
      source: 'oidc',
      oidcIdToken: accepted.idToken,
      userAgent: where.userAgent,
      ip: where.ip,
    });
    await tx
      .update(schema.factor)
      .set({ usedAt: sql`now()` })
      .where(and(eq(schema.factor.identityId, identity.id), eq(schema.factor.kind, 'oidc')));
    await recordAudit(tx, {
      actorPersonId: personId,
      command: 'sign-in',
      targetKind: 'identity',
      targetId: identity.id,
      payload: { source: 'oidc' },
    });
    return minted.token;
  });

  const next = /^\/[A-Za-z0-9\-._~/]*$/.test(checks.next) ? checks.next : '/app';
  const response = NextResponse.redirect(new URL(next, request.url), 302);
  response.cookies.set(sessionCookieName(), token, sessionCookieOptions());
  response.cookies.set(OIDC_COOKIE, '', {
    path: '/auth/oidc',
    maxAge: 0,
    httpOnly: true,
    secure: isProduction(),
  });
  return response;
}
