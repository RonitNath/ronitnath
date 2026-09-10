/* The `identity` row that is no longer a door.
 *
 * Signing in is better-auth's business: the address, the password and the
 * ZITADEL subject all live in `auth.user` and `auth.account`, and nothing in
 * this app authenticates from the `identity` table any more. What that table
 * still holds is *handles* — the address, number or bare name one member typed
 * when they held somebody — and the match queue that compares the two
 * (`src/features/people/matches.ts`) is written in terms of identity ids on
 * both sides of a pair.
 *
 * So a member's own confirmed address keeps a row here, as a record rather
 * than as a credential: it has no factor, nothing reads it to let anybody in,
 * and its only job is to be the other half of a proposal. Keeping it is much
 * cheaper than teaching the match queue to compare two different kinds of
 * thing, and it is what makes "somebody is holding a contact card with your
 * address on it" keep working across the move.
 *
 * The stamp lands on sign-in rather than at the moment of confirmation:
 * better-auth confirms an address at its own endpoint, and a successful
 * sign-in is proof that the confirmation happened. */

import { and, eq, isNull, sql } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import { proposeMatchesFor } from '@/features/people/matches';
import { normaliseEmail } from '@/lib/fleet/identity';

/** Record a member's address against their person, and — the first time it is
 *  known to be confirmed — ask whether anybody is holding a contact card that
 *  spells it. Safe to call on every sign-in: everything here is an upsert or a
 *  guarded update, and the proposal is written once. */
export async function mirrorIdentity(input: {
  userId: string;
  email: string;
  verified: boolean;
  source: 'local' | 'oidc';
}): Promise<void> {
  const subject = normaliseEmail(input.email);
  await database().transaction(async (tx) => {
    const people = await tx
      .select({ id: schema.person.id })
      .from(schema.person)
      .where(and(eq(schema.person.userId, input.userId), isNull(schema.person.mergedInto)))
      .limit(1);
    const personId = people[0]?.id;
    if (personId === undefined) return;

    const existing = await tx
      .select({ id: schema.identity.id, verifiedAt: schema.identity.verifiedAt })
      .from(schema.identity)
      .where(and(eq(schema.identity.source, input.source), eq(schema.identity.subject, subject)))
      .limit(1);

    let identityId = existing[0]?.id;
    const wasVerified = existing[0]?.verifiedAt !== null && existing[0]?.verifiedAt !== undefined;
    if (identityId === undefined) {
      const rows = await tx
        .insert(schema.identity)
        .values({
          personId,
          source: input.source,
          subject,
          verifiedAt: input.verified ? sql`now()` : null,
        })
        .returning({ id: schema.identity.id });
      identityId = rows[0]!.id;
    } else if (input.verified && !wasVerified) {
      await tx
        .update(schema.identity)
        .set({ verifiedAt: sql`now()` })
        .where(eq(schema.identity.id, identityId));
    }

    /* A newly confirmed address may be the address somebody else wrote on a
     * contact card. Proposing is not merging: what this writes is a question
     * for the person who just proved the address. */
    if (input.verified && !wasVerified) {
      await proposeMatchesFor(tx, { identityId, personId, subject });
    }
  });
}
