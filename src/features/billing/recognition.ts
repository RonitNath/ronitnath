import { and, eq, sql } from 'drizzle-orm';
import { postJournal, type LedgerClient } from '@isoastra/fleet-ledger';
import type { BillingChart } from '@isoastra/fleet-billing/accounting';
import { database, schema } from '@/db/client';
import { dateOnly } from './model';

type BillingTx = Parameters<Parameters<ReturnType<typeof database>['transaction']>[0]>[0];
export type AppChart = BillingChart & { deferredRevenue: string };
export type RecognitionState = { totalAtoms: bigint; creditedAtoms: bigint; refundedAtoms: bigint; recognizedAtoms: bigint };

export function remainingRecognition(state: RecognitionState) {
  const atoms = state.totalAtoms - state.creditedAtoms - state.refundedAtoms - state.recognizedAtoms;
  return atoms > 0n ? atoms : 0n;
}

export function adjustmentAccount(state: Pick<RecognitionState, 'recognizedAtoms'> & { fulfilledAt: Date | null }, chart: AppChart) {
  return state.recognizedAtoms > 0n || state.fulfilledAt ? chart.contraRevenue : chart.deferredRevenue;
}

export async function recognizeCompletedOrder(tx: BillingTx, input: { orderId: string; bookId: string; chart: AppChart; operationKey: string; evidenceId: string; now: Date }) {
  await tx.execute(sql`SELECT 1 FROM fleet_ledger_book WHERE id=${input.bookId} FOR UPDATE`);
  const state = (await tx.select({
    totalAtoms: schema.billingOrder.totalAtoms,
    creditedAtoms: schema.billingOrder.creditedAtoms,
    refundedAtoms: schema.billingOrder.refundedAtoms,
    recognizedAtoms: schema.billingOrder.recognizedAtoms,
  }).from(schema.billingOrder)
    .innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id))
    .where(and(eq(schema.billingOrder.id, input.orderId), eq(schema.billingSeller.bookId, input.bookId)))
    .limit(1)
    .for('update'))[0];
  if (!state) throw new Error('order');
  const atoms = remainingRecognition(state);
  if (atoms > 0n) await postJournal(tx as unknown as LedgerClient, { bookId: input.bookId, operationKey: input.operationKey, kind: 'billing.revenue.recognition', occurredAt: input.now, effectiveOn: dateOnly(input.now), evidence: [{ kind: 'fulfillment', id: input.evidenceId }], policy: { key: 'billing.fulfillment', version: 2 }, postings: [{ accountId: input.chart.deferredRevenue, unit: 'USD', atoms }, { accountId: input.chart.revenue, unit: 'USD', atoms: -atoms }] });
  if (atoms > 0n) {
    const changed = await tx.update(schema.billingOrder).set({ recognizedAtoms: state.recognizedAtoms + atoms, recognizedAt: input.now }).where(and(eq(schema.billingOrder.id, input.orderId), eq(schema.billingOrder.recognizedAtoms, state.recognizedAtoms))).returning({ id: schema.billingOrder.id });
    if (!changed.length) throw new Error('stale');
  }
  return atoms;
}
