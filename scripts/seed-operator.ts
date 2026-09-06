/* `pnpm seed:operator --email ronit@isoastra.com`
 *
 * Creates the person that ZITADEL will attach itself to, and grants it
 * `operator` on `platform:*`. No factors and no identity: the OIDC callback
 * links the subject on first sign-in, and an identity seeded here would have
 * to guess a `sub` it cannot know. Running it twice changes nothing.
 *
 * It is a convenience rather than a requirement — the callback provisions the
 * same person if this never runs — but seeding it means the display name and
 * the operator grant exist before anyone signs in, which is what makes a first
 * deploy inspectable. */

import { config } from 'dotenv';
import { and, eq } from 'drizzle-orm';

config({ path: ['.env.local', '.env'], quiet: true });

async function main() {
  const { database, schema } = await import('../src/db/client');
  const { createPerson, grantOperator, seedSubject } = await import(
    '../src/features/auth/provision'
  );
  const { normalizeEmail, oidcAllowlist } = await import('../src/features/auth/email-address');

  const flag = process.argv.indexOf('--email');
  const email = normalizeEmail(
    flag >= 0 ? (process.argv[flag + 1] ?? '') : (oidcAllowlist()[0] ?? ''),
  );
  if (!email) throw new Error('usage: pnpm seed:operator --email <address>');
  if (!oidcAllowlist().includes(email)) {
    throw new Error(`${email} is not on OIDC_ALLOWLIST; it could never sign in`);
  }

  const db = database();
  const marker = seedSubject(email);
  const existing = await db
    .select({ personId: schema.identity.personId })
    .from(schema.identity)
    .where(and(eq(schema.identity.source, 'oidc'), eq(schema.identity.subject, marker)))
    .limit(1);

  const personId = await db.transaction(async (tx) => {
    if (existing[0]) {
      await grantOperator(tx, existing[0].personId);
      return existing[0].personId;
    }
    const id = await createPerson(tx, { displayName: email.split('@')[0] ?? email });
    /* A placeholder subject so a second run recognises its own work. The
     * callback links the real `sub` as a second identity on this person. */
    await tx
      .insert(schema.identity)
      .values({ personId: id, source: 'oidc', subject: marker });
    await grantOperator(tx, id);
    await tx.insert(schema.audit).values({
      actorPersonId: id,
      command: 'seed-operator',
      targetKind: 'party',
      targetId: id,
      payload: { email },
    });
    return id;
  });

  console.log(JSON.stringify({ level: 'info', event: 'seed.operator', email, personId }));
  process.exit(0);
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});
