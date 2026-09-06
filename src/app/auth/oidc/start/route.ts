/* Start the ZITADEL round trip. The one-time checks go into a ten-minute
 * HttpOnly cookie rather than a server-side store: they belong to this one
 * browser for this one redirect, and a store would be a second thing to expire
 * and to share between processes. */

import { NextResponse } from 'next/server';

import { OIDC_COOKIE, OIDC_COOKIE_MAX_AGE, authorizationRequest } from '@/features/auth/oidc';
import { isProduction } from '@/lib/env';

export const dynamic = 'force-dynamic';

export async function GET(request: Request) {
  const query = new URL(request.url).searchParams;
  const next = query.get('next') ?? '/app';
  const { url, checks } = await authorizationRequest(next, query.get('reauth') === '1');

  const response = NextResponse.redirect(url.toString(), 302);
  response.cookies.set(OIDC_COOKIE, JSON.stringify(checks), {
    httpOnly: true,
    secure: isProduction(),
    sameSite: 'lax',
    path: '/auth/oidc',
    maxAge: OIDC_COOKIE_MAX_AGE,
  });
  return response;
}
