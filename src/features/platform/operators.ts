/* Who holds god mode, and the one rule about taking it away.
 *
 * The relation is `person → operator → platform:*`, the same row
 * `grantOperator` writes for the seeded operator and the OIDC callback keeps
 * true. Revoking is allowed, including of yourself — but never the last one:
 * a deployment with no operator is a deployment nobody can fix, and that is
 * the same shape as "the last owner cannot leave" (src/lib/authority.ts). */

import { and, eq } from 'drizzle-orm';

import { schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';
import { OPERATOR_RESOURCE } from '@/features/auth/principal';

export async function operatorPersonIds(tx: Transaction): Promise<number[]> {
  const rows = await tx
    .select({ id: schema.relation.subjectId })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.verb, 'operator'),
        eq(schema.relation.resourceKind, OPERATOR_RESOURCE.kind),
        eq(schema.relation.resourceId, OPERATOR_RESOURCE.id),
      ),
    );
  return rows.map((row) => row.id);
}

/** Whether revoking this person would leave the deployment with none. Pure,
 *  because it is the property this file exists to hold. */
export function isLastOperator(operators: readonly number[], personId: number): boolean {
  return operators.includes(personId) && operators.length <= 1;
}

/** Whether the revocation may proceed at all. A person who does not hold it
 *  is not a refusal about the last one — it is nothing to revoke. */
export function refusesRevoke(operators: readonly number[], personId: number): 'last' | 'no' | null {
  if (!operators.includes(personId)) return 'no';
  return isLastOperator(operators, personId) ? 'last' : null;
}

/** The one relation the platform tier grants: `person → operator → platform:*`.
 *  Idempotent, because a seed script and a first sign-in may both write it. */
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
