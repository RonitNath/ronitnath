/* `pnpm seed:operator --email ronit@isoastra.com [--password <secret>]`
 *
 * Makes the account that runs the platform tier exist before anybody signs in,
 * so that a first deploy is inspectable.
 *
 * Two shapes, one command. With `--password` it seeds a local operator: a
 * user, a `credential` account holding the argon2id hash, a person, and the
 * relation `person → operator → platform:*`. That is what a deployment being
 * tested and the Playwright run need — an operator they can sign in as without
 * ZITADEL. Without one it seeds Ronit's own operator, which has no password
 * and never will: a user row carrying the allowlisted address and nothing
 * else, so that the first ZITADEL sign-in links its account to *this* user
 * (`accountLinking.trustedProviders` includes `zitadel`) rather than making a
 * second person nobody meant.
 *
 * Everything here is derived and upserted — the user id is a hash of the
 * address — so running it twice changes nothing except the password, if one
 * was named. */

import { config } from 'dotenv';
import { and, eq, isNull } from 'drizzle-orm';

config({ path: ['.env.local', '.env'], quiet: true });

async function main(): Promise<void> {
  const { createLocalAccountIssuer } = await import('better-auth');
  const { authSchema, database, schema } = await import('../src/db/client');
  const { hashPassword, identityId, normaliseEmail } = await import('../src/lib/fleet/identity');
  const { oidcAllowlist } = await import('../src/features/auth/email-address');
  const { grantOperator } = await import('../src/features/platform/operators');
  const { createPerson } = await import('../src/features/people/provision');

  const flag = process.argv.indexOf('--email');
  const email = normaliseEmail(
    flag >= 0 ? (process.argv[flag + 1] ?? '') : (oidcAllowlist()[0] ?? ''),
  );
  if (!email) throw new Error('usage: pnpm seed:operator --email <address> [--password <secret>]');

  const passwordFlag = process.argv.indexOf('--password');
  const password = passwordFlag >= 0 ? (process.argv[passwordFlag + 1] ?? '') : '';
  if (!password && !oidcAllowlist().includes(email)) {
    throw new Error(`${email} is not on OIDC_ALLOWLIST and has no password; it could never sign in`);
  }

  const db = database();
  const userId = identityId(email);
  const name = email.split('@')[0] ?? email;

  await db
    .insert(authSchema.user)
    .values({ id: userId, name, email, emailVerified: true })
    .onConflictDoNothing({ target: authSchema.user.id });

  if (password) {
    const phc = await hashPassword(password);
    await db
      .insert(authSchema.account)
      .values({
        id: identityId(`credential:${email}`),
        /* The library's own value for a password account, not the site's
         * origin: `/sign-in/email` looks the account up by it. */
        issuer: createLocalAccountIssuer('credential'),
        accountId: userId,
        providerId: 'credential',
        userId,
        password: phc,
      })
      .onConflictDoUpdate({
        target: authSchema.account.id,
        set: { password: phc, issuer: createLocalAccountIssuer('credential'), updatedAt: new Date() },
      });
  }

  const personId = await db.transaction(async (tx) => {
    const existing = await tx
      .select({ id: schema.person.id })
      .from(schema.person)
      .where(and(eq(schema.person.userId, userId), isNull(schema.person.mergedInto)))
      .limit(1);
    const id = existing[0]?.id ?? (await createPerson(tx, { displayName: name, userId }));
    await grantOperator(tx, id);
    await tx.insert(schema.audit).values({
      actorPersonId: id,
      command: 'seed-operator',
      targetKind: 'party',
      targetId: id,
      payload: { email, door: password ? 'local' : 'oidc' },
    });
    return id;
  });

  console.log(
    JSON.stringify({
      level: 'info',
      event: 'seed.operator',
      email,
      personId,
      userId,
      door: password ? 'local' : 'oidc',
    }),
  );
  process.exit(0);
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});
