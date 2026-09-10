/* Making a person exist. A person *is* a party — the two rows share an id —
 * so nothing that hands out an id has to say which of the two it meant, and a
 * relation can name a person and an organization the same way.
 *
 * This used to live beside the door, back when creating a person and creating
 * an account were the same act. They are not: better-auth creates users, and a
 * person is who somebody is to everyone else here whether or not they have
 * ever signed in. A guest a host held by email has a person and no user; the
 * day they claim it, `person.user_id` is filled in and nothing else changes. */

import { schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';

export async function createPerson(
  tx: Transaction,
  input: { displayName: string; held?: boolean; userId?: string | null },
): Promise<number> {
  const parties = await tx
    .insert(schema.party)
    .values({ kind: 'person' })
    .returning({ id: schema.party.id });
  const id = parties[0]!.id;
  await tx.insert(schema.person).values({
    id,
    displayName: input.displayName,
    held: input.held ?? false,
    userId: input.userId ?? null,
  });
  return id;
}
