/* The audit row. One per command, written inside that command's own
 * transaction — so a mutation that is not in the feed did not happen, and a
 * feed row without its mutation cannot exist.
 *
 * Impersonation is handled here and nowhere else. While an operator is signed
 * in as somebody, the principal *is* that person, so `actorPersonId` stays
 * what the command passed; the operator answerable for it is added to the
 * payload by this one function, and every command therefore names both
 * without any of them remembering to (docs/plan.md ladder R6). */

import { schema } from '@/db/client';
import type { Transaction } from './db';

export interface AuditRow {
  actorPersonId?: number | null;
  command: string;
  targetKind?: string | null;
  targetId?: number | null;
  payload?: Record<string, unknown> | null;
}

/** The key an impersonated command's payload carries. */
export const ACTING_OPERATOR = 'acting_operator';

/** Who is answerable for what this request is doing, when that is not the
 *  principal. Null everywhere else, including outside a request — a script or
 *  a unit test has no cookie jar, and asking for one must not throw. */
async function actingOperator(): Promise<number | null> {
  try {
    const { currentPrincipal } = await import('./session');
    return (await currentPrincipal())?.actingOperatorId ?? null;
  } catch {
    return null;
  }
}

export async function recordAudit(tx: Transaction, row: AuditRow): Promise<void> {
  const operator = await actingOperator();
  const payload =
    operator === null ? (row.payload ?? null) : { ...row.payload, [ACTING_OPERATOR]: operator };
  await tx.insert(schema.audit).values({
    actorPersonId: row.actorPersonId ?? null,
    command: row.command,
    targetKind: row.targetKind ?? null,
    targetId: row.targetId ?? null,
    payload,
  });
}
