'use server';

/* The door's commands, now a thin layer over `auth.api`.
 *
 * Better-auth owns what these used to do by hand: the throttles, the tokens,
 * the letters, the cookie, the session row and the argon2id verification. What
 * is left here is what a door on *this* site has to keep — the uniform answer,
 * the rule that an allowlisted address has no password path at all, the
 * re-hash that quietly upgrades an old PHC string, and the `identity` mirror
 * the match queue reads.
 *
 * Two rules run through all of them. Registration and password reset answer
 * the same way whether or not the address is known, so neither form is a
 * lookup service for who has an account here; and an address on
 * `OIDC_ALLOWLIST` is reached through ZITADEL or not at all — which is also
 * refused without saying so.
 *
 * Every call goes through `auth.api` rather than `fetch`, so the `nextCookies`
 * plugin is what writes the cookie: a server action cannot set one any other
 * way, and better-auth's own rate limiting only applies to requests that
 * arrive over HTTP, which is why the counters that matter here are the ones
 * the library keeps on `/api/auth/*` and the edge keeps in front of it. */

import { and, eq, isNull } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { headers } from 'next/headers';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { encodeId } from '@/lib/ids';
import { LEGACY_APP_ROOT, userPath, userRoute } from '@/lib/paths';
import { auth, rehashOnSignIn } from './auth';
import { isAllowlisted, looksLikeEmail, normalizeEmail } from './email-address';
import { AUTH_FAILED, CHECK_INBOX, RESET_SENT, type FormState } from './form-state';
import { mirrorIdentity } from './mirror';
import { currentPrincipal } from './principal';
import { hashToken } from './secrets';

const PASSWORD_MIN = 10;
const PASSWORD_MAX = 200;

/* Where better-auth's own letters land. The reset link is a GET on
 * `/api/auth/reset-password/<token>` that checks the token and bounces to
 * this path carrying it; the verification link verifies and bounces here. */
const RESET_PAGE = '/auth/reset';
const VERIFY_PAGE = '/auth/verify';

const email = z.string().trim().max(254).transform(normalizeEmail).refine(looksLikeEmail);
const password = z.string().min(PASSWORD_MIN).max(PASSWORD_MAX);

const signUpInput = z.object({
  displayName: z.string().trim().min(1).max(120),
  email,
  password,
});
const signInInput = z.object({ email, password: z.string().max(PASSWORD_MAX), next: z.string() });
const addressInput = z.object({ email });
const resetInput = z.object({ token: z.string().min(1), password });

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

/* Where a `next=` may send someone: this site, by path, and never back to the
 * door it just came from. Not exported — everything a `'use server'` module
 * exports is a callable endpoint, and a predicate does not need to be one. */
function safeNext(raw: string, fallback: string): string {
  return /^\/(?!\/|auth(\/|$))[A-Za-z0-9\-._~/]*$/.test(raw) ? raw : fallback;
}

/** Where somebody lands when they asked for nothing in particular: their own
 *  surfaces, which are addressed by their public id and so cannot be named by
 *  a constant. A user with no person behind it has no indoors to go to and
 *  gets the landing. */
async function homeOf(userId: string): Promise<string> {
  const rows = await database()
    .select({ personId: schema.person.id })
    .from(schema.person)
    .where(and(eq(schema.person.userId, userId), isNull(schema.person.mergedInto)))
    .limit(1);
  const row = rows[0];
  return row ? userPath(encodeId('person', row.personId)) : '/';
}

/* Where an OAuth round trip may land. Wider than `safeNext` by exactly one
 * path: `/auth/confirmed` is the re-authentication landing, which is on this
 * site, carries only a `next` of its own, and is the one auth route a
 * callback is allowed to name. */
function safeCallback(raw: string, fallback: string): string {
  if (/^\/auth\/confirmed\?next=%2Fo%2Fisoastra[A-Za-z0-9%\-._~]*$/.test(raw)) return raw;
  return safeNext(raw, fallback);
}

/* Better-auth's refusals arrive as its `APIError`, and `instanceof` is not the
 * way to recognise one: the class is built by `makeErrorForHideStackFrame`,
 * the endpoints throw the copy bundled with the library, and the copy reached
 * through `better-auth/api` is a different object — the check silently fails
 * and a refusal escapes as a 500. What is stable is the shape: a `statusCode`
 * and a `body.code`, which is also the only part of it this file reads. */
function errorCode(error: unknown): string | null {
  const shaped = error as { statusCode?: unknown; body?: { code?: unknown } } | null;
  if (!shaped || typeof shaped.statusCode !== 'number') return null;
  const code = shaped.body?.code;
  return typeof code === 'string' ? code : 'DECLINED';
}

/* ------------------------------------------------------------ registration */

/** SignUp. The letter is better-auth's; the answer is ours and is the same
 *  one whether the address was free, taken, or refused outright. */
export async function signUpEmail(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = signUpInput.safeParse({
    displayName: field(form, 'displayName'),
    email: field(form, 'email'),
    password: field(form, 'password'),
  });
  if (!parsed.success) {
    return {
      form: 'register',
      error: `A name, an address, and a password of at least ${PASSWORD_MIN} characters.`,
    };
  }
  const { displayName, email: address, password: secret } = parsed.data;

  /* Allowlisted addresses sign in through ZITADEL and never hold a password.
   * Refused here, silently, exactly like a taken address. */
  if (!isAllowlisted(address)) {
    try {
      const created = await auth().api.signUpEmail({
        body: {
          email: address,
          password: secret,
          name: displayName,
          callbackURL: VERIFY_PAGE,
        },
        headers: await headers(),
      });
      /* The address is on record from the moment the account exists; it is
       * stamped confirmed at the first sign-in, which is the proof. */
      await mirrorIdentity({
        userId: created.user.id,
        email: address,
        verified: false,
        source: 'local',
      });
    } catch (error) {
      /* A taken address is not something this form is allowed to reveal, and
       * a transport that failed is a line in the log rather than a hint. */
      if (errorCode(error) === null) throw error;
    }
  }

  return { form: 'register', notice: CHECK_INBOX };
}

/* ------------------------------------------------------------- signing in */

/** SignIn. Three answers, not two: a session, a refusal, and an address whose
 *  password is right and whose confirmation never landed — which the door may
 *  say, because the password proved who is asking. */
export async function signInEmail(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = signInInput.safeParse({
    email: field(form, 'email'),
    password: field(form, 'password'),
    next: field(form, 'next'),
  });
  if (!parsed.success) return { form: 'sign-in', error: AUTH_FAILED };
  const { email: address, password: secret, next } = parsed.data;

  if (isAllowlisted(address)) return { form: 'sign-in', error: AUTH_FAILED };

  let userId: string;
  try {
    const signed = await auth().api.signInEmail({
      body: { email: address, password: secret },
      headers: await headers(),
    });
    userId = signed.user.id;
  } catch (error) {
    const code = errorCode(error);
    if (code === null) throw error;
    if (code === 'EMAIL_NOT_VERIFIED') {
      return { form: 'sign-in', unverified: true, email: address };
    }
    return { form: 'sign-in', error: AUTH_FAILED };
  }

  /* Both of these are for the *next* sign-in, not this one: the account is
   * already through the door. */
  await rehashOnSignIn(address, secret);
  await mirrorIdentity({ userId, email: address, verified: true, source: 'local' });

  redirect(safeNext(next, await homeOf(userId)));
}

/** The other door. Better-auth registers the ZITADEL config as a provider, so
 *  the start of the round trip is `signInSocial` and the callback is the
 *  library's own `/api/auth/callback/zitadel`; the allowlist is enforced in
 *  the factory, where the profile arrives. */
export async function signInWithIsoastra(_prev: FormState, form: FormData): Promise<FormState> {
  /* The round trip has not happened yet, so there is nobody to send home:
   * `/app` is the one path that resolves "me" from the session it is given,
   * and it permanently redirects to `/u/<me>` the moment the callback lands. */
  const next = safeCallback(field(form, 'next'), LEGACY_APP_ROOT);
  let url: string;
  try {
    const started = await auth().api.signInSocial({
      body: { provider: 'zitadel', callbackURL: next },
      headers: await headers(),
    });
    if (!started.url) return { form: 'sign-in', error: AUTH_FAILED };
    url = started.url;
  } catch (error) {
    if (errorCode(error) === null) throw error;
    return { form: 'sign-in', error: AUTH_FAILED };
  }
  redirect(url);
}

/* -------------------------------------------------------------- letters */

/** RequestPasswordReset. Answered the same way whichever address is typed. */
export async function requestPasswordReset(
  _prev: FormState,
  form: FormData,
): Promise<FormState> {
  const parsed = addressInput.safeParse({ email: field(form, 'email') });
  if (parsed.success && !isAllowlisted(parsed.data.email)) {
    try {
      await auth().api.requestPasswordReset({
        body: { email: parsed.data.email, redirectTo: RESET_PAGE },
        headers: await headers(),
      });
    } catch (error) {
      if (errorCode(error) === null) throw error;
    }
  }
  return { form: 'reset', notice: RESET_SENT };
}

/** ResetPassword. The token arrives in the URL, put there by better-auth's own
 *  link after it checked that the token is live. Every other session goes with
 *  the reset: whoever held one may be the reason for it. */
export async function resetPassword(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = resetInput.safeParse({
    token: field(form, 'token'),
    password: field(form, 'password'),
  });
  if (!parsed.success) {
    return { error: `A password of at least ${PASSWORD_MIN} characters.` };
  }
  try {
    await auth().api.resetPassword({
      body: { token: parsed.data.token, newPassword: parsed.data.password },
      headers: await headers(),
    });
  } catch (error) {
    if (errorCode(error) === null) throw error;
    return { error: 'That link has been used or has expired.' };
  }
  return { notice: 'Password set. Every other session has been signed out.' };
}

/** RequestVerification. The letter sign-up sent may never have arrived — the
 *  transport can fail after the account exists — so the door can send another
 *  one, answered the same way whether or not the address is one that could be
 *  confirmed. */
export async function requestVerification(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = addressInput.safeParse({ email: field(form, 'email') });
  if (parsed.success && !isAllowlisted(parsed.data.email)) {
    try {
      await auth().api.sendVerificationEmail({
        body: { email: parsed.data.email, callbackURL: VERIFY_PAGE },
        headers: await headers(),
      });
    } catch (error) {
      if (errorCode(error) === null) throw error;
    }
  }
  return {
    form: 'sign-in',
    unverified: true,
    email: parsed.success ? parsed.data.email : undefined,
    notice: CHECK_INBOX,
  };
}

/* -------------------------------------------------------------- sessions */

/** SignOut. Better-auth revokes the row and clears the cookie; the redirect
 *  is ours, because a door that leaves you on the page you were refused from
 *  has not finished. */
export async function signOut(): Promise<void> {
  try {
    await auth().api.signOut({ headers: await headers() });
  } catch (error) {
    if (errorCode(error) === null) throw error;
  }
  redirect('/');
}

/** RevokeSession. What crosses the wire is the SHA-256 of the session token,
 *  never the token: the token *is* the key to that session, and a page that
 *  prints one has handed it out. The digest is enough to name a row. */
export async function revokeSession(_prev: FormState, form: FormData): Promise<FormState> {
  const principal = await currentPrincipal();
  if (!principal) redirect('/auth/sign-in');
  const wanted = field(form, 'session');
  if (wanted.length === 0) return { error: 'That session is not one you can end here.' };

  const request = { headers: await headers() };
  const sessions = await auth().api.listSessions(request);
  const target = sessions.find((row) => hashToken(row.token) === wanted);
  if (!target || target.id === principal.sessionId) {
    return { error: 'That session is not one you can end here.' };
  }

  await auth().api.revokeSession({ body: { token: target.token }, ...request });
  revalidatePath(userRoute('sessions'), 'page');
  return { notice: 'Session ended.' };
}
