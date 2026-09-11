/* The door, and everything behind it.
 *
 * Better-auth owns the tables, the endpoints under `/api/auth/*`, the cookie,
 * the CSRF check, the OAuth state and PKCE, and the rate limits. What is left
 * here is the four things a fleet has to keep for itself: how a password is
 * hashed, how an address is written down, who is allowed through the ZITADEL
 * door, and the fact that every `auth.user` gets a `person` row on this site
 * because guests, events and rsvps all name people rather than users.
 *
 * Version 1.7.2 is pinned fleet-wide and the changelog is read before it
 * moves: 1.7.0 scoped `account` by issuer, and a schema surprise in the module
 * that owns sign-in is the expensive kind. Only three plugins are loaded —
 * genericOAuth, organization, nextCookies — because the advisory stream in
 * this library has concentrated in sso, oidc-provider, scim and mcp, none of
 * which this site has any use for.
 *
 * `baseURL` and `trustedOrigins` come from PUBLIC_ORIGIN, never from
 * `request.url`: behind the edge that is the container's bind address, and an
 * OIDC redirect_uri built from it points at 0.0.0.0.
 *
 * `cookieCache` is off. It would save a query per request and cost immediate
 * revocation, and this site has a SignInAs command whose whole safety story is
 * that ending it ends it now. */

import { betterAuth } from 'better-auth';
import { drizzleAdapter } from 'better-auth/adapters/drizzle';
import { nextCookies } from 'better-auth/next-js';
import { genericOAuth, organization } from 'better-auth/plugins';
import { eq } from 'drizzle-orm';

import { authSchema, database, schema } from '@/db/client';
import { publicOrigin, required } from '@/lib/env';
import { hashPassword, isCurrent, normaliseEmail, verifyPassword } from '@/lib/fleet/identity';
import { deliver } from '@/lib/mail';
import { passwordResetMail, verificationMail } from '@/lib/mail';

/** Who ZITADEL is allowed to vouch for. Employee SSO is for employees; the
 *  ruling is one address, and an empty list means the door is shut rather than
 *  open (fleet-conventions §2). */
function oidcAllowlist(): string[] {
  return (process.env.OIDC_ALLOWLIST ?? '')
    .split(/[,\s]+/)
    .map((entry) => normaliseEmail(entry))
    .filter((entry) => entry.length > 0);
}

function oidcConfigured(): boolean {
  return Boolean(process.env.OIDC_ISSUER && process.env.OIDC_CLIENT_ID);
}

/* The person row. A user is how somebody signs in; a person is who they are to
 * everyone else on this site, and a guest who has never signed in has one
 * already. Linking is therefore an upsert on the address, not an insert:
 * somebody a host held by email months ago becomes a member by having their
 * held person claimed rather than by getting a second one. */
async function ensurePerson(user: { id: string; email: string; name?: string | null }) {
  const db = database();
  await db.transaction(async (tx) => {
    const existing = await tx
      .select({ id: schema.person.id })
      .from(schema.person)
      .where(eq(schema.person.userId, user.id))
      .limit(1);
    if (existing.length > 0) return;

    const parties = await tx
      .insert(schema.party)
      .values({ kind: 'person' })
      .returning({ id: schema.party.id });
    const id = parties[0]!.id;
    await tx.insert(schema.person).values({
      id,
      displayName: user.name?.trim() || user.email,
      held: false,
      userId: user.id,
    });
  });
}

/* Built on first use, never at import. The build loads every route module to
 * collect its page data, and a route module must not need a database URL or a
 * signing secret to exist — the same rule the rest of this repo follows for
 * `pool()` and `required()`. Next reloads modules in development, so the
 * instance is parked on `globalThis` rather than rebuilt on every save. */
const globalForAuth = globalThis as unknown as { rnAuth?: ReturnType<typeof build> };

function build() {
  return betterAuth({
    appName: 'ronitnath',
    baseURL: publicOrigin(),
    basePath: '/api/auth',
    secret: process.env.AUTH_SECRET ?? required('AUTH_SECRET'),
    trustedOrigins: [publicOrigin()],
    database: drizzleAdapter(database(), {
      provider: 'pg',
      schema: { ...authSchema },
      usePlural: false,
    }),
    user: {
      additionalFields: {
        role: { type: 'string', required: false, input: false },
        disabledAt: { type: 'date', required: false, input: false },
      },
    },
    session: {
      expiresIn: 30 * 24 * 60 * 60,
      updateAge: 24 * 60 * 60,
      cookieCache: { enabled: false },
      additionalFields: {
        /* Set only while an operator is signed in as somebody else. The session
         * *is* the target's; this is who is answerable for it, and it is a
         * session fact rather than a second cookie so that ending the session
         * ends the impersonation. */
        actingOperatorId: { type: 'string', required: false, input: false },
        /* When a password or a fresh round trip was last presented. The commands
         * that destroy or impersonate ask for one inside ten minutes. */
        reauthenticatedAt: { type: 'date', required: false, input: false },
      },
    },
    emailAndPassword: {
      enabled: true,
      requireEmailVerification: true,
      revokeSessionsOnPasswordReset: true,
      password: {
        hash: (password) => hashPassword(password),
        /* The PHC string carries its own parameters, so this verifies a hash
         * made under any of them — including the m=19456,t=2,p=1 hashes this
         * site wrote before the fleet standard existed, which is what lets
         * scripts/migrate-identity.ts copy them across untouched. */
        verify: ({ hash, password }) => verifyPassword(hash, password),
      },
      sendResetPassword: async ({ user, url }) => {
        await deliver(passwordResetMail(user.email, url));
      },
    },
    emailVerification: {
      autoSignInAfterVerification: true,
      sendVerificationEmail: async ({ user, url }) => {
        await deliver(verificationMail(user.email, url));
      },
    },
    account: {
      accountLinking: { enabled: true, trustedProviders: ['zitadel'] },
    },
    rateLimit: { storage: 'database' },
    advanced: {
      ipAddress: { ipAddressHeaders: ['x-forwarded-for'] },
      useSecureCookies: process.env.NODE_ENV === 'production',
    },
    databaseHooks: {
      user: {
        create: {
          before: async (user) => {
            const email = normaliseEmail(user.email);
            return { data: { ...user, email } };
          },
          after: async (user) => {
            await ensurePerson(user);
          },
        },
        update: {
          before: async (user) => {
            if (typeof user.email !== 'string') return { data: user };
            return { data: { ...user, email: normaliseEmail(user.email) } };
          },
        },
      },
      session: {
        create: {
          /* A retired party keeps every row it had and loses its door. Without
           * this the door still opens: better-auth knows nothing about
           * `party.disabled_at`, so a disabled person could present the right
           * password, be handed a session, and only then be turned away by
           * `currentPrincipal` — which confirms to them that their password is
           * still good and leaves a session row behind. Refusing here makes the
           * sign-in decline like any other wrong answer.
           *
           * The party is asked rather than a copy of the fact in
           * `user.disabled_at`, because retirement is something this site knows
           * about a person and two copies of it would be two things to keep in
           * step. */
          before: async (session) => {
            const rows = await database()
              .select({ disabledAt: schema.party.disabledAt })
              .from(schema.person)
              .innerJoin(schema.party, eq(schema.party.id, schema.person.id))
              .where(eq(schema.person.userId, session.userId))
              .limit(1);
            if (rows[0]?.disabledAt) return false;
            return { data: session };
          },
        },
      },
    },
    plugins: [
      ...(oidcConfigured()
        ? [
            genericOAuth({
              config: [
                {
                  providerId: 'zitadel',
                  discoveryUrl: `${required('OIDC_ISSUER')}/.well-known/openid-configuration`,
                  clientId: required('OIDC_CLIENT_ID'),
                  clientSecret: required('OIDC_CLIENT_SECRET'),
                  scopes: ['openid', 'email', 'profile'],
                  pkce: true,
                  /* The one address the ruling allows through this door. An
                   * unverified address is refused outright: ZITADEL is trusted
                   * to say who somebody is, not to guess. */
                  mapProfileToUser: (profile: Record<string, unknown>) => {
                    const email = normaliseEmail(String(profile.email ?? ''));
                    const verified = profile.email_verified === true;
                    if (!verified || !oidcAllowlist().includes(email)) {
                      throw new Error('not allowed here');
                    }
                    return { email, name: String(profile.name ?? email) };
                  },
                },
              ],
            }),
          ]
        : []),
      organization(),
      /* Last, always: it is what turns better-auth's Set-Cookie into a cookie a
       * server action has actually written. */
      nextCookies(),
    ],
  });
}

export function auth(): ReturnType<typeof build> {
  globalForAuth.rnAuth ??= build();
  return globalForAuth.rnAuth;
}

export type Auth = ReturnType<typeof build>;

/** Re-hash a password that verified under old parameters. Called from the
 *  sign-in action on success, outside the request's critical path: the account
 *  is already through the door, and the point is that the *next* sign-in costs
 *  what today's parameters cost. */
export async function rehashOnSignIn(email: string, password: string): Promise<void> {
  const db = database();
  const normalised = normaliseEmail(email);
  const rows = await db
    .select({ id: authSchema.account.id, password: authSchema.account.password })
    .from(authSchema.account)
    .innerJoin(authSchema.user, eq(authSchema.user.id, authSchema.account.userId))
    .where(eq(authSchema.user.email, normalised))
    .limit(10);
  for (const row of rows) {
    if (!row.password || isCurrent(row.password)) continue;
    if (!(await verifyPassword(row.password, password))) continue;
    await db
      .update(authSchema.account)
      .set({ password: await hashPassword(password), updatedAt: new Date() })
      .where(eq(authSchema.account.id, row.id));
  }
}
