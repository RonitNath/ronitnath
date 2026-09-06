/* ProposeMatch. Somebody confirms an address; somebody else's contact card
 * spells the same address; those two may be one person.
 *
 * Proposing is not merging and never becomes merging on its own. What this
 * writes is a question, addressed to the person who just proved the address —
 * the one side of the pair who can answer it without anyone's permission. The
 * operator's half (ruling on a pair where neither side is the asker, and
 * splitting one back apart) is R6; it reads the same rows and this file is
 * the seam.
 *
 * Only an address proposes. A phone handle would too if there were a
 * confirmed phone factor to compare it against, and a bare name never will —
 * two people called Ravi are two people. */

import { and, eq, isNull, ne } from 'drizzle-orm';

import { schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { Transaction } from '@/features/auth/db';
import { CONTACT } from './authority';

/* Queue order for R6, in hundredths. A confirmed address is strong evidence
 * and still not a merge. */
export const SCORE = { verified_email: 85, claimed_link: 70 } as const;

/** Called when an identity becomes verified. Writes one proposal per held
 *  person whose handle spells the same subject and whom somebody else holds.
 *  Returns how many were written. */
export async function proposeMatchesFor(
  tx: Transaction,
  input: { identityId: number; personId: number; subject: string },
): Promise<number> {
  const candidates = await tx
    .select({ identityId: schema.identity.id, personId: schema.identity.personId })
    .from(schema.identity)
    .innerJoin(schema.person, eq(schema.person.id, schema.identity.personId))
    .where(
      and(
        eq(schema.identity.source, 'handle'),
        eq(schema.identity.subject, input.subject),
        eq(schema.person.held, true),
        isNull(schema.person.mergedInto),
        ne(schema.identity.personId, input.personId),
      ),
    );
  if (candidates.length === 0) return 0;

  /* A contact card nobody holds is nobody's claim to make. */
  const holders = await tx
    .select({ heldId: schema.relation.resourceId, holderId: schema.relation.subjectId })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.verb, CONTACT),
        eq(schema.relation.resourceKind, 'person'),
      ),
    );
  const heldBySomeoneElse = new Set(
    holders.filter((row) => row.holderId !== input.personId).map((row) => row.heldId),
  );

  let written = 0;
  for (const candidate of candidates) {
    if (!heldBySomeoneElse.has(candidate.personId)) continue;
    const [identityA, identityB] = [input.identityId, candidate.identityId].sort((a, b) => a - b);
    const rows = await tx
      .insert(schema.match)
      .values({
        identityA: identityA!,
        identityB: identityB!,
        signal: 'verified_email',
        score: SCORE.verified_email,
        evidence: `confirmed address ${input.subject}`,
      })
      /* A signal seen twice is the same question, not a new one. */
      .onConflictDoNothing()
      .returning({ id: schema.match.id });
    if (rows.length === 0) continue;
    written += 1;
    await recordAudit(tx, {
      actorPersonId: input.personId,
      command: 'propose-match',
      targetKind: 'match',
      targetId: rows[0]!.id,
      payload: { signal: 'verified_email', identity_a: identityA, identity_b: identityB },
    });
  }
  return written;
}

/** Settle every open question about a pair of people that this merge has
 *  just answered. */
export async function settleMatchesFor(
  tx: Transaction,
  input: { matchId: number; personId: number },
): Promise<void> {
  await tx
    .update(schema.match)
    .set({ status: 'confirmed', decidedAt: new Date(), decidedBy: input.personId })
    .where(and(eq(schema.match.id, input.matchId), eq(schema.match.status, 'proposed')));
}
