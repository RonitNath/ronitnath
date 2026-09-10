/* Every endpoint better-auth owns, mounted at `/api/auth/*`: sign-up, sign-in,
 * sign-out, the verification and reset round trips, the OAuth callback, and
 * session listing and revocation. The library routes inside this one handler;
 * nothing here decides anything.
 *
 * The handler is built inside the exported functions rather than at module
 * scope. `next build` loads every route module to collect its page data, and a
 * route module must not need a database URL or a signing secret to exist —
 * the same rule `pool()` and `required()` follow. */

import { toNextJsHandler } from 'better-auth/next-js';

import { auth } from '@/features/auth/auth';

export function GET(request: Request): Promise<Response> {
  return toNextJsHandler(auth()).GET(request);
}

export function POST(request: Request): Promise<Response> {
  return toNextJsHandler(auth()).POST(request);
}
