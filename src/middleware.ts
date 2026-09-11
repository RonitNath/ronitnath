/* The redirect, and nothing else.
 *
 * Better-auth owns session expiry now: the cookie carries its own maximum age
 * and the row behind it carries the real one, and the library slides both when
 * it resolves a session. There is nothing left for an edge middleware to
 * re-stamp, so this does the one job a middleware is the right place for —
 * sending somebody with no cookie at all to the door, from the one place that
 * knows the whole path a layout cannot read.
 *
 * It is a convenience, never the guard. `getSessionCookie` reads presence, not
 * validity: it does not verify the signature, does not ask the database, and
 * a forged cookie gets exactly as far as the page, where `requireUser` decides
 * for real. That is the shape better-auth's own documentation asks for, and it
 * is why this file needs neither a database nor the Node runtime. */

import { getSessionCookie } from 'better-auth/cookies';
import { NextResponse, type NextRequest } from 'next/server';

/* `/u` is a person's own surfaces and `/app` the stub that forwards the old
 * paths to them; both want a reader who is somebody, and both can say so from
 * here without learning anything a stranger should not know.
 *
 * `/o` is deliberately not here. `/o/isoastra` is an operator surface, and an
 * operator surface asked for by a stranger must be the 404 that a page which
 * does not exist gives — a redirect to the door would tell them the address
 * is real. The pages under `/o` decide for themselves: an organization sends
 * a visitor to the door carrying the handle, the console gives the 404. */
const GUARDED = ['/app', '/u/'];

function guarded(pathname: string): boolean {
  return GUARDED.some((prefix) => pathname === prefix.replace(/\/$/, '') || pathname.startsWith(prefix));
}

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;
  if (!guarded(pathname)) return NextResponse.next();
  if (getSessionCookie(request)) return NextResponse.next();

  /* An event stream is not a document and must never be answered with a
   * redirect: an `EventSource` follows it, is handed the sign-in page, fails
   * to parse it, and reconnects every three seconds for as long as the tab is
   * open. The routes themselves decline the same way, with the same 404 a
   * stranger gets for a stream that is not theirs; this is the edge saying it
   * first, before a route handler is even reached. */
  if (request.headers.get('accept')?.includes('text/event-stream')) {
    return new NextResponse(null, { status: 404 });
  }

  /* Absolute from PUBLIC_ORIGIN, never from `request.url`: behind the edge
   * that is the container's bind address (fleet-conventions §1). */
  const door = new URL('/auth/sign-in', process.env.PUBLIC_ORIGIN ?? request.url);
  door.searchParams.set('next', pathname);
  return NextResponse.redirect(door);
}

export const config = {
  /* Documents only, and only the guarded prefixes — a static asset carries no
   * session and has nothing to be redirected about. */
  matcher: ['/app/:path*', '/u/:path*'],
};
