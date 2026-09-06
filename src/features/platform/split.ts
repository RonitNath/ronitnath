/* Split: a merge was wrong, and the two people are two people again.
 *
 * R3 made this possible by never destroying the absorbed person. Everything
 * it did was either a row it moved (identities, owned resources, sessions), a
 * row it added to the survivor (the copied relation edges), or the one column
 * it wrote last (`person.merged_into`) — and it wrote every one of those ids
 * into its own audit row. That row is the undo record; this file reads it and
 * reverses it, statement for statement.
 *
 * The one thing it will not do quietly: if the survivor has *since* gained a
 * factor on an identity that came across in the merge — a password set on the
 * absorbed person's address after the two became one — then pulling that
 * identity back takes a live door with it. The operator is told, and may say
 * so anyway; that answer is what lands in the audit. */

import { and, eq, inArray, isNull, sql } from 'drizzle-orm';
import { z } from 'zod';

import { schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { Transaction } from '@/features/auth/db';

/** What `mergePersons` wrote down about what it did. */
export const undoRecord = z.object({
  survivor: z.number().int().positive(),
  absorbed: z.number().int().positive(),
  moved_identities: z.array(z.number().int().positive()).default([]),
  dropped_identities: z
    .array(z.object({ source: z.enum(['local', 'oidc', 'handle']), subject: z.string() }))
    .default([]),
  copied_relations: z.array(z.number().int().positive()).default([]),
  moved_resources: z.array(z.number().int().positive()).default([]),
  moved_sessions: z.array(z.number().int().positive()).default([]),
});

export type UndoRecord = z.infer<typeof undoRecord>;

export function readUndo(payload: unknown): UndoRecord | null {
  const parsed = undoRecord.safeParse(payload);
  return parsed.success ? parsed.data : null;
}

export interface FactorRow {
  identityId: number;
  kind: string;
  createdAt: Date;
}

/** The factors that would leave with the identities this split pulls back, and
 *  that were not there when the merge happened. Pure, because "what a split
 *  costs" is the property this file exists to state. */
export function factorsAtRisk(
  factors: readonly FactorRow[],
  moved: readonly number[],
  mergedAt: Date,
): FactorRow[] {
  return factors.filter(
    (row) => moved.includes(row.identityId) && row.createdAt.getTime() > mergedAt.getTime(),
  );
}

export type SplitOutcome =
  | { ok: true; restored: number }
  | { ok: false; reason: 'no-record' | 'not-merged' }
  | { ok: false; reason: 'factors'; at_risk: number };

/** Reverse one merge inside the caller's transaction. `auditId` names the
 *  `confirm-match` row to undo: an operator splits a particular merge, not a
 *  person, and a person merged twice has two of them. */
export async function splitMerge(
  tx: Transaction,
  input: { auditId: number; actorPersonId: number; reason: string; confirmed: boolean },
): Promise<SplitOutcome> {
  const rows = await tx
    .select({ id: schema.audit.id, payload: schema.audit.payload, at: schema.audit.at })
    .from(schema.audit)
    .where(and(eq(schema.audit.id, input.auditId), eq(schema.audit.command, 'confirm-match')))
    .limit(1);
  const row = rows[0];
  const undo = row ? readUndo(row.payload) : null;
  if (!row || !undo) return { ok: false, reason: 'no-record' };

  const retired = await tx
    .select({ id: schema.person.id })
    .from(schema.person)
    .where(and(eq(schema.person.id, undo.absorbed), eq(schema.person.mergedInto, undo.survivor)))
    .limit(1);
  if (retired.length === 0) return { ok: false, reason: 'not-merged' };

  if (undo.moved_identities.length > 0) {
    const factors = await tx
      .select({
        identityId: schema.factor.identityId,
        kind: schema.factor.kind,
        createdAt: schema.factor.createdAt,
      })
      .from(schema.factor)
      .where(inArray(schema.factor.identityId, undo.moved_identities));
    const risky = factorsAtRisk(factors, undo.moved_identities, row.at);
    if (risky.length > 0 && !input.confirmed) {
      return { ok: false, reason: 'factors', at_risk: risky.length };
    }
  }

  /* Everything the merge moved, moved back. */
  if (undo.moved_identities.length > 0) {
    await tx
      .update(schema.identity)
      .set({ personId: undo.absorbed })
      .where(
        and(
          inArray(schema.identity.id, undo.moved_identities),
          eq(schema.identity.personId, undo.survivor),
        ),
      );
  }
  if (undo.dropped_identities.length > 0) {
    /* The duplicates the merge deleted. They were the same address spelled
     * twice, so the survivor keeps its own and the absorbed person gets its
     * note back. */
    await tx
      .insert(schema.identity)
      .values(
        undo.dropped_identities.map((row_) => ({
          personId: undo.absorbed,
          source: row_.source,
          subject: row_.subject,
        })),
      )
      .onConflictDoNothing();
  }
  if (undo.copied_relations.length > 0) {
    await tx.delete(schema.relation).where(inArray(schema.relation.id, undo.copied_relations));
  }
  if (undo.moved_resources.length > 0) {
    await tx
      .update(schema.resource)
      .set({ ownerPartyId: undo.absorbed })
      .where(inArray(schema.resource.id, undo.moved_resources));
  }
  if (undo.moved_sessions.length > 0) {
    await tx
      .update(schema.session)
      .set({ personId: undo.absorbed })
      .where(inArray(schema.session.id, undo.moved_sessions));
  }

  /* And the two columns that made the person stop resolving. */
  await tx
    .update(schema.person)
    .set({ mergedInto: null })
    .where(and(eq(schema.person.id, undo.absorbed), eq(schema.person.mergedInto, undo.survivor)));
  await tx
    .update(schema.party)
    .set({ disabledAt: null })
    .where(eq(schema.party.id, undo.absorbed));

  /* A question answered by a merge that has been taken back is not open
   * again — the operator has just said these are two people. */
  if (undo.moved_identities.length > 0) {
    await tx
      .update(schema.match)
      .set({ status: 'rejected', decidedAt: sql`now()`, decidedBy: input.actorPersonId })
      .where(
        and(
          eq(schema.match.status, 'confirmed'),
          inArray(schema.match.identityA, undo.moved_identities),
        ),
      );
    await tx
      .update(schema.match)
      .set({ status: 'rejected', decidedAt: sql`now()`, decidedBy: input.actorPersonId })
      .where(
        and(
          eq(schema.match.status, 'confirmed'),
          inArray(schema.match.identityB, undo.moved_identities),
        ),
      );
  }

  await recordAudit(tx, {
    actorPersonId: input.actorPersonId,
    command: 'split-merge',
    targetKind: 'person',
    targetId: undo.absorbed,
    payload: {
      survivor: undo.survivor,
      absorbed: undo.absorbed,
      merge_audit: input.auditId,
      reason: input.reason,
      forced: input.confirmed,
      identities_restored: undo.moved_identities.length,
      relations_removed: undo.copied_relations.length,
    },
  });
  return { ok: true, restored: undo.moved_identities.length };
}

/** Whether a person is retired into another one — what the parties list turns
 *  into the word `merged`. */
export async function isRetired(tx: Transaction, personId: number): Promise<boolean> {
  const rows = await tx
    .select({ id: schema.person.id })
    .from(schema.person)
    .where(and(eq(schema.person.id, personId), isNull(schema.person.mergedInto)))
    .limit(1);
  return rows.length === 0;
}
