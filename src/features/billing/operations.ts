import { createHash } from 'node:crypto';
import { and, eq, sql } from 'drizzle-orm';
import { database, schema } from '@/db/client';

type BillingTx = Parameters<Parameters<ReturnType<typeof database>['transaction']>[0]>[0];
const canonical = (value: unknown): string => {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  return `{${Object.entries(value as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b)).map(([key, item]) => `${JSON.stringify(key)}:${canonical(item)}`).join(',')}}`;
};

export async function beginCommand(tx: BillingTx, input: { operationKey: string; actorPersonId: number; command: string; request: Record<string, unknown> }) {
  if (!input.operationKey || input.operationKey.length > 200) throw new Error('operation key');
  const requestText = canonical(input.request), requestHash = createHash('sha256').update(requestText).digest('hex');
  const existing = (await tx.select().from(schema.billingCommand).where(eq(schema.billingCommand.operationKey, input.operationKey)).limit(1))[0];
  if (existing) {
    if (existing.actorPersonId !== input.actorPersonId || existing.command !== input.command || existing.requestHash !== requestHash) throw new Error('changed retry');
    return { retry: true, result: existing.result } as const;
  }
  const inserted = await tx.insert(schema.billingCommand).values({ operationKey: input.operationKey, actorPersonId: input.actorPersonId, command: input.command, requestHash, request: input.request }).onConflictDoNothing().returning({ operationKey: schema.billingCommand.operationKey });
  if (inserted.length) return { retry: false, result: null } as const;
  const raced = (await tx.select().from(schema.billingCommand).where(eq(schema.billingCommand.operationKey, input.operationKey)).limit(1))[0];
  if (!raced || raced.actorPersonId !== input.actorPersonId || raced.command !== input.command || raced.requestHash !== requestHash) throw new Error('changed retry');
  return { retry: true, result: raced.result } as const;
}

export async function completeCommand(tx: BillingTx, operationKey: string, result: Record<string, unknown>) {
  const rows = await tx.update(schema.billingCommand).set({ result }).where(and(eq(schema.billingCommand.operationKey, operationKey), sql`${schema.billingCommand.result} IS NULL`)).returning({ operationKey: schema.billingCommand.operationKey });
  if (!rows.length) throw new Error('operation receipt');
}
