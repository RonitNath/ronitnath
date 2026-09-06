/* The cookie half of the sliding window.
 *
 * The database slides `expires_at` when a session is resolved, but a Server
 * Component cannot write a cookie, so without this the browser's copy would
 * still expire thirty days after it was minted no matter how often it was
 * used. Re-stamping the same value on each navigation is the whole job: no
 * database, no session lookup, nothing that has to run on Node. */

import { NextResponse, type NextRequest } from 'next/server';

export function middleware(request: NextRequest) {
  const name = process.env.SESSION_COOKIE ?? 'rn_session';
  const token = request.cookies.get(name)?.value;

  /* The member tier's redirect, sent from the one place that knows the whole
   * path: a layout cannot read it, so `requireMember` inside one can only
   * name itself. This is a convenience, not the guard — `requireMember` still
   * decides, and a cookie that resolves to nothing is turned away there. */
  if (!token && request.nextUrl.pathname.startsWith('/app')) {
    const door = new URL('/auth', process.env.PUBLIC_ORIGIN ?? request.url);
    door.searchParams.set('next', request.nextUrl.pathname);
    return NextResponse.redirect(door);
  }

  const response = NextResponse.next();
  if (token) {
    const days = Number(process.env.SESSION_TTL_DAYS ?? 30);
    response.cookies.set(name, token, {
      httpOnly: true,
      secure: process.env.NODE_ENV === 'production',
      sameSite: 'lax',
      path: '/',
      maxAge: (Number.isFinite(days) && days > 0 ? days : 30) * 24 * 60 * 60,
    });
  }
  return response;
}

export const config = {
  /* Documents only. Static assets carry no session and re-stamping a cookie on
   * each of them would be a Set-Cookie header on every image on the page. */
  matcher: ['/((?!_next/static|_next/image|favicon.ico|fonts/|healthz).*)'],
};
