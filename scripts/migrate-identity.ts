/* `pnpm migrate:identity`
 *
 * Copies the doors this site already had into the identity core better-auth
 * now owns. It is the data half of the move; the schema half is migrations
 * 0008–0010, which created `auth.*` and `person.user_id`.
 *
 * Three rules make it a copy rather than a rewrite:
 *
 *  - The password hash is copied **verbatim**. A PHC string carries its own
 *    parameters (`$argon2id$v=19$m=19456,t=2,p=1$…` is what this site wrote
 *    before the fleet standard existed), and the `password.verify` override in
 *    features/auth/auth.ts reads them from the string. Nobody is asked to
 *    reset anything, and the first sign-in quietly re-hashes under today's
 *    parameters.
 *  - Every id is derived, not random: `identityId(seed)` is a hash of the
 *    address or the subject, so running this twice writes the same rows twice
 *    and the second run changes nothing. That is the whole idempotence story.
 *  - `identity` rows with `source='handle'` get nothing. A handle is one
 *    member's note about how to reach somebody — an address, a number, a bare
 *    name — and it authenticates nobody. Held people are people, not doors.
 *
 * An OIDC identity becomes an `account` with `providerId='zitadel'` and
 * `accountId` = the ZITADEL `sub`, linked to the user with the same address
 * when there is one. A `seed:` subject, the placeholder the old seed script
 * filed before anyone knew the real `sub`, is not a subject ZITADEL will ever
 * present and is skipped.
 *
 * Sessions are not copied — everyone signs in again — and nothing is dropped
 * here: the old tables stay until somebody has looked at the result. */

import { config } from 'dotenv';
import { and, eq, inArray, isNotNull, isNull, ne } from 'drizzle-orm';

config({ path: ['.env.local', '.env'], quiet: true });

interface Summary {
  users: number;
  credentials: number;
  federated: number;
  linked: number;
  skippedHandles: number;
  skippedUnverified: number;
  skippedSeeds: number;
}

/** Point a person at its user, unless it already has one or that user is
 *  already somebody else's. `person.user_id` is unique — one account is one
 *  person — so this is a guard rather than an upsert. */
async function attach(
  db: Awaited<ReturnType<typeof import('../src/db/client')['database']>>,
  personId: number,
  userId: string,
): Promise<number> {
  const { schema } = await import('../src/db/client');
  const taken = await db
    .select({ id: schema.person.id })
    .from(schema.person)
    .where(eq(schema.person.userId, userId))
    .limit(1);
  if (taken[0]) return 0;
  const rows = await db
    .update(schema.person)
    .set({ userId })
    .where(and(eq(schema.person.id, personId), isNull(schema.person.userId)))
    .returning({ id: schema.person.id });
  return rows.length;
}

async function main(): Promise<void> {
  const { createLocalAccountIssuer } = await import('better-auth');
  const { authSchema, database, schema } = await import('../src/db/client');
  const { identityId, normaliseEmail } = await import('../src/lib/fleet/identity');
  const { publicOrigin } = await import('../src/lib/env');

  const db = database();
  /* Not the site's origin. Since 1.7.0 an account is scoped by the issuer that
   * vouched for it, and for a password the library's own value is the literal
   * `local:credential` — sign-in looks the account up by it, so a credential
   * row filed under anything else is a door that does not open. Verified
   * against `createLocalAccountIssuer` in @better-auth/core, which is where
   * `/sign-in/email` gets it. */
  const credentialIssuer = createLocalAccountIssuer('credential');
  const summary: Summary = {
    users: 0,
    credentials: 0,
    federated: 0,
    linked: 0,
    skippedHandles: 0,
    skippedUnverified: 0,
    skippedSeeds: 0,
  };

  const identities = await db
    .select({
      id: schema.identity.id,
      personId: schema.identity.personId,
      source: schema.identity.source,
      subject: schema.identity.subject,
      verifiedAt: schema.identity.verifiedAt,
      createdAt: schema.identity.createdAt,
      displayName: schema.person.displayName,
    })
    .from(schema.identity)
    .innerJoin(schema.person, eq(schema.person.id, schema.identity.personId))
    .where(isNull(schema.person.mergedInto))
    .orderBy(schema.identity.id);

  summary.skippedHandles = identities.filter((row) => row.source === 'handle').length;

  /* The password each local identity holds, in one query rather than one per
   * identity: a migration that is slow enough to be interrupted is a
   * migration somebody will interrupt. */
  const localIds = identities.filter((row) => row.source === 'local').map((row) => row.id);
  const secrets = new Map<number, string>();
  if (localIds.length > 0) {
    const factors = await db
      .select({ identityId: schema.factor.identityId, secret: schema.factor.secret })
      .from(schema.factor)
      .where(
        and(
          inArray(schema.factor.identityId, localIds),
          eq(schema.factor.kind, 'password'),
          isNotNull(schema.factor.secret),
        ),
      );
    for (const row of factors) if (row.secret) secrets.set(row.identityId, row.secret);
  }

  /* ---------------------------------------------------------- local doors */

  /* An address is one user, whichever person happens to hold it: the user id
   * is derived from the address, so two identity rows spelling the same
   * address converge rather than collide. */
  const userIdOf = new Map<string, string>();

  for (const row of identities) {
    if (row.source !== 'local') continue;
    const email = normaliseEmail(row.subject);
    if (row.verifiedAt === null) {
      summary.skippedUnverified += 1;
      continue;
    }
    /* An address that already has a user keeps it. That is not only the
     * re-run case: an account created through better-auth since the schema
     * landed has a random id, and the derived one would collide on `email`
     * rather than on `id`. Asking first is what makes this a copy of what is
     * missing rather than an insert that assumes an empty table. */
    const already = await db
      .select({ id: authSchema.user.id })
      .from(authSchema.user)
      .where(eq(authSchema.user.email, email))
      .limit(1);
    const userId = already[0]?.id ?? identityId(email);
    userIdOf.set(email, userId);

    if (!already[0]) {
      await db.insert(authSchema.user).values({
        id: userId,
        name: row.displayName,
        email,
        emailVerified: true,
        createdAt: row.verifiedAt,
        updatedAt: new Date(),
      });
    }
    summary.users += 1;

    const phc = secrets.get(row.id);
    if (phc) {
      await db
        .insert(authSchema.account)
        .values({
          id: identityId(`credential:${email}`),
          issuer: credentialIssuer,
          accountId: userId,
          providerId: 'credential',
          userId,
          /* Verbatim. The verify override reads the parameters off it. */
          password: phc,
        })
        /* Bare, not by id: an account for this user through this provider may
         * already exist under an id better-auth chose. */
        .onConflictDoNothing();
      summary.credentials += 1;
    }

    summary.linked += await attach(db, row.personId, userId);
  }

  /* --------------------------------------------------------- ZITADEL doors */

  for (const row of identities) {
    if (row.source !== 'oidc') continue;
    if (row.subject.startsWith('seed:')) {
      summary.skippedSeeds += 1;
      continue;
    }

    /* Which user this subject belongs to: the one holding the same address if
     * this person has a confirmed one, otherwise a user of its own, made from
     * the person's name so that the account has somebody to be. */
    const sibling = identities.find(
      (other) =>
        other.source === 'local' &&
        other.personId === row.personId &&
        other.verifiedAt !== null,
    );
    const email = sibling ? normaliseEmail(sibling.subject) : null;
    let userId = email ? userIdOf.get(email) : undefined;

    if (!userId) {
      userId = identityId(`zitadel:${row.subject}`);
      await db
        .insert(authSchema.user)
        .values({
          id: userId,
          name: row.displayName,
          /* No confirmed address on file: the subject is the only thing that
           * is certainly this account's, and better-auth requires the column.
           * The first ZITADEL sign-in overwrites it with the real one. */
          email: email ?? `${row.subject}@zitadel.invalid`,
          emailVerified: email !== null,
          createdAt: row.createdAt,
          updatedAt: new Date(),
        })
        .onConflictDoNothing();
      summary.users += 1;
    }

    await db
      .insert(authSchema.account)
      .values({
        id: identityId(`zitadel:account:${row.subject}`),
        /* For a genericOAuth provider the library files the account under the
         * *discovered* issuer, which for ZITADEL is `OIDC_ISSUER` itself. */
        issuer: process.env.OIDC_ISSUER ?? publicOrigin(),
        accountId: row.subject,
        providerId: 'zitadel',
        userId,
      })
      .onConflictDoNothing();
    summary.federated += 1;

    summary.linked += await attach(db, row.personId, userId);
  }

  /* Repair, not migration: rows this script wrote before the issuer was read
   * off the library rather than out of the brief carry the site's origin, and
   * every one of them is a password nobody can sign in with. Narrowed to
   * `credential`, where the correct value is a constant, so a row better-auth
   * wrote itself is updated to what it already says. */
  const repaired = await db
    .update(authSchema.account)
    .set({ issuer: credentialIssuer })
    .where(
      and(
        eq(authSchema.account.providerId, 'credential'),
        ne(authSchema.account.issuer, credentialIssuer),
      ),
    )
    .returning({ id: authSchema.account.id });

  console.log(
    JSON.stringify({
      level: 'info',
      event: 'migrate.identity',
      ...summary,
      repairedIssuers: repaired.length,
    }),
  );
  process.exit(0);
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});
