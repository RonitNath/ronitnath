/* The audit row. One per command, written inside that command's own
 * transaction — so a mutation that is not in the feed did not happen, and a
 * feed row without its mutation cannot exist. */

import { schema } from '@/db/client';
import type { Transaction } from './db';

export interface AuditRow {
  actorPersonId?: number | null;
  command: string;
  targetKind?: string | null;
  targetId?: number | null;
  payload?: Record<string, unknown> | null;
}

export async function recordAudit(tx: Transaction, row: AuditRow): Promise<void> {
  await tx.insert(schema.audit).values({
    actorPersonId: row.actorPersonId ?? null,
    command: row.command,
    targetKind: row.targetKind ?? null,
    targetId: row.targetId ?? null,
    payload: row.payload ?? null,
  });
}
