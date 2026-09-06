/* The ZITADEL door. Authorization code with PKCE S256, state and nonce, and
 * an allowlist of exactly one address.
 *
 * The three one-time values do not go into the database: they belong to one
 * browser for one minute, so they ride in a short-lived HttpOnly cookie that
 * the callback reads and deletes. Discovery is done once per process and
 * cached — it is a network round trip, and every sign-in would otherwise pay
 * for it twice.
 *
 * What the callback will accept is narrow on purpose: an id_token whose
 * `email_verified` is true and whose `email` is on `OIDC_ALLOWLIST`. Anything
 * else gets the same decline page and writes no rows at all — not a person,
 * not an identity, not an audit trail of who tried, because a stranger's
 * address is not this site's to keep. */

import * as client from 'openid-client';

import { publicOrigin, required } from '@/lib/env';
import { isAllowlisted, normalizeEmail } from './email-address';

export interface OidcChecks {
  state: string;
  nonce: string;
  codeVerifier: string;
  next: string;
  /* A round trip asked for by ReAuthenticate rather than by a sign-in: the
   * OP is told to prove it again (`prompt=login`, `max_age=0`) and the
   * session this mints is stamped (src/features/platform/reauth.ts). */
  reauth?: boolean;
}

export const OIDC_COOKIE = 'rn_oidc';
export const OIDC_COOKIE_MAX_AGE = 600;

const globalForOidc = globalThis as unknown as { rnOidc?: Promise<client.Configuration> };

export function configuration(): Promise<client.Configuration> {
  globalForOidc.rnOidc ??= client.discovery(
    new URL(required('OIDC_ISSUER')),
    required('OIDC_CLIENT_ID'),
    required('OIDC_CLIENT_SECRET'),
  );
  return globalForOidc.rnOidc;
}

export function redirectUri(): string {
  return required('OIDC_REDIRECT_URI');
}

/** Mint the checks and the URL that carries their public halves. */
export async function authorizationRequest(
  next: string,
  reauth = false,
): Promise<{ url: URL; checks: OidcChecks }> {
  const config = await configuration();
  const codeVerifier = client.randomPKCECodeVerifier();
  const checks: OidcChecks = {
    state: client.randomState(),
    nonce: client.randomNonce(),
    codeVerifier,
    next,
    ...(reauth ? { reauth: true } : {}),
  };
  const url = client.buildAuthorizationUrl(config, {
    redirect_uri: redirectUri(),
    scope: 'openid profile email',
    code_challenge: await client.calculatePKCECodeChallenge(codeVerifier),
    code_challenge_method: 'S256',
    state: checks.state,
    nonce: checks.nonce,
    /* An existing OP session must not answer this one silently: a
     * re-authentication that nobody was asked for proves nothing. */
    ...(reauth ? { prompt: 'login', max_age: '0' } : {}),
  });
  return { url, checks };
}

export interface OidcSubject {
  subject: string;
  email: string;
  displayName: string;
  idToken: string | null;
}

/** Exchange the code and decide whether this is a subject we know.
 *  `null` is the uniform decline: wrong person, unverified address, or a
 *  response that failed any of the three checks. */
export async function acceptCallback(
  currentUrl: URL,
  checks: OidcChecks,
): Promise<OidcSubject | null> {
  const config = await configuration();
  let tokens;
  try {
    tokens = await client.authorizationCodeGrant(config, currentUrl, {
      pkceCodeVerifier: checks.codeVerifier,
      expectedState: checks.state,
      expectedNonce: checks.nonce,
      idTokenExpected: true,
    });
  } catch {
    return null;
  }

  const claims = tokens.claims();
  if (!claims?.sub) return null;
  const address = typeof claims.email === 'string' ? normalizeEmail(claims.email) : '';
  if (claims.email_verified !== true || !address || !isAllowlisted(address)) return null;

  const name = typeof claims.name === 'string' && claims.name.trim() ? claims.name.trim() : address;
  return {
    subject: claims.sub,
    email: address,
    displayName: name,
    idToken: tokens.id_token ?? null,
  };
}

/** Where to send the browser so ZITADEL forgets it too. `null` when the OP
 *  publishes no end_session endpoint, in which case sign-out is local only. */
export async function endSessionUrl(idToken: string | null): Promise<string | null> {
  try {
    const config = await configuration();
    if (!config.serverMetadata().end_session_endpoint) return null;
    const url = client.buildEndSessionUrl(config, {
      post_logout_redirect_uri: `${publicOrigin()}/`,
      ...(idToken ? { id_token_hint: idToken } : {}),
    });
    return url.toString();
  } catch {
    return null;
  }
}
