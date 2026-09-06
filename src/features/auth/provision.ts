/* Making a person exist. A person *is* a party — the two rows share an id —
 * so nothing that hands out an id has to say which of the two it meant, and a
 * relation can name a person and an organization the same way. */

import { and, eq } from 'drizzle-orm';

import { schema } from '@/db/client';
import type { Transaction } from './db';
import { OPERATOR_RESOURCE } from './session';

export async function createPerson(
  tx: Transaction,
  input: { displayName: string; held?: boolean },
): Promise<number> {
  const parties = await tx
    .insert(schema.party)
    .values({ kind: 'person' })
    .returning({ id: schema.party.id });
  const id = parties[0]!.id;
  await tx
    .insert(schema.person)
    .values({ id, displayName: input.displayName, held: input.held ?? false });
  return id;
}

/** The one relation this rung grants: `person → operator → platform:*`.
 *  Idempotent, because the OIDC callback runs it on a sign-in that may not be
 *  the first one. */
export async function grantOperator(tx: Transaction, personId: number): Promise<void> {
  await tx
    .insert(schema.relation)
    .values({
      subjectKind: 'person',
      subjectId: personId,
      verb: 'operator',
      resourceKind: OPERATOR_RESOURCE.kind,
      resourceId: OPERATOR_RESOURCE.id,
    })
    .onConflictDoNothing();
}

export async function findIdentity(
  tx: Transaction,
  source: 'local' | 'oidc',
  subject: string,
): Promise<{ id: number; personId: number; verifiedAt: Date | null } | null> {
  const rows = await tx
    .select({
      id: schema.identity.id,
      personId: schema.identity.personId,
      verifiedAt: schema.identity.verifiedAt,
    })
    .from(schema.identity)
    .where(and(eq(schema.identity.source, source), eq(schema.identity.subject, subject)))
    .limit(1);
  return rows[0] ?? null;
}

/** The placeholder subject `pnpm seed:operator` files an identity under, so
 *  that a person can be seeded before anyone knows the `sub` ZITADEL will
 *  issue. The first real sign-in overwrites it. */
export function seedSubject(email: string): string {
  return `seed:${email}`;
}
