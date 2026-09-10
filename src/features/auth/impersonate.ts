/* Wearing somebody else's session.
 *
 * SignInAs is not a second cookie and not a flag on the operator's own
 * session: it is an ordinary better-auth session belonging to the target,
 * carrying one extra column that says who is answerable for it. Nothing in
 * `allows` learns a new case — the principal really is the target person — and
 * ending the session ends the impersonation, which is the whole safety story
 * (it is also why `cookieCache` is off in the factory).
 *
 * Better-auth has no endpoint for this. The admin plugin has one and is not
 * loaded: the fleet contract pins core + genericOAuth + organization, and the
 * plugin brings a ban/role model this site already answers with its own
 * `relation` table. So the session is minted through the library's own
 * internal adapter — the same call `/sign-in/email` makes — and the cookie is
 * signed the way better-call signs it.
 *
 * The signature is the one piece that is written out rather than imported.
 * better-call's `signCookieValue` is internal, and the two re-exports that
 * reach it (`serializeSignedCookie` from `better-call`, re-exported by
 * `better-auth`) exist in the type declarations but not in the runtime bundle
 * — Next says so at build time. What it does is small, exact, and pinned by
 * the test beside this file: HMAC-SHA256 over the token with the auth secret,
 * base64 of the raw digest, appended after a dot. Next percent-encodes a
 * cookie value on the way out, which is the encoding better-call's parser
 * expects, so nothing here encodes it a second time. */

import { createHmac } from 'node:crypto';
import { getCookies } from 'better-auth/cookies';
import { cookies } from 'next/headers';

import { auth } from './auth';

/** The cookie value better-auth will accept for this token: `<token>.<sig>`,
 *  the signature being base64 of HMAC-SHA256(token) under the auth secret. */
export function signCookieValue(value: string, secret: string): string {
  const signature = createHmac('sha256', secret).update(value).digest('base64');
  return `${value}.${signature}`;
}

/** Mint a session for `userId`. Returns its id; nothing is worn yet, because
 *  the cookie has to be written after everything that reads a session — see
 *  the ordering note in features/platform/impersonation.ts. */
export async function mintSession(
  userId: string,
  actingOperatorId: number | null,
): Promise<string> {
  const context = await auth().$context;
  const session = await context.internalAdapter.createSession(userId, false, {
    /* A text column, because better-auth's own ids are text. The reader is
     * `features/auth/principal.ts`, which turns it back into a person id. */
    actingOperatorId: actingOperatorId === null ? null : String(actingOperatorId),
  });
  return session.id;
}

/** Put a minted session in the reader's cookie jar. Called last in a command,
 *  because better-auth clears this cookie whenever it is asked to resolve the
 *  one still in the request headers and cannot. */
export async function wearCookieFor(sessionId: string): Promise<void> {
  const context = await auth().$context;
  const rows = await context.adapter.findMany<{ token: string }>({
    model: 'session',
    where: [{ field: 'id', value: sessionId }],
    limit: 1,
  });
  const token = rows[0]?.token;
  if (!token) throw new Error('the session to wear has gone');

  const cookie = getCookies(auth().options).sessionToken;
  const jar = await cookies();
  jar.set(cookie.name, signCookieValue(token, context.secret), {
    httpOnly: true,
    secure: cookie.attributes.secure,
    sameSite: 'lax',
    path: '/',
    maxAge: auth().options.session?.expiresIn ?? 30 * 24 * 60 * 60,
  });
}

/** End one session by its id, without touching the cookie. Used on both sides
 *  of an impersonation: the operator's own session ends when they put somebody
 *  else's on, and the worn one ends when they take it off. */
export async function endSession(sessionId: string): Promise<void> {
  const context = await auth().$context;
  /* By id rather than by token: the id is what an audit row can carry and
   * what `Principal` holds, and a token is a credential this side should not
   * be passing around to name a row. */
  await context.adapter.delete({ model: 'session', where: [{ field: 'id', value: sessionId }] });
}
