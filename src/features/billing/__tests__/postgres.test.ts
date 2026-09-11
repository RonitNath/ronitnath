import { randomUUID } from 'node:crypto';
import { eq } from 'drizzle-orm';
import { describe, expect, it } from 'vitest';
import { issueInvoice, recordPayment, type BillingChart } from '@isoastra/fleet-billing/accounting';
import type { LedgerClient } from '@isoastra/fleet-ledger';
import { database, schema } from '@/db/client';

const enabled = Boolean(process.env.BILLING_DATABASE_TEST);
const suite = enabled ? describe : describe.skip;

suite('billing application transaction contract', () => {
  it('commits the app order and ledger invoice together, then allocates excess to credit', async () => {
    const person = await database().transaction(async (tx) => {
      const party = (await tx.insert(schema.party).values({ kind: 'person' }).returning({ id: schema.party.id }))[0]!;
      await tx.insert(schema.person).values({ id: party.id, displayName: 'Billing Test' });
      return party.id;
    });
    const seller = (await database().select().from(schema.billingSeller).where(eq(schema.billingSeller.id, 'ronit')).limit(1))[0]!;
    const chart = seller.chart as BillingChart;
    const orderId = randomUUID(), invoiceId = `test:${orderId}`, operationKey = `test:${orderId}`;
    await database().transaction(async (tx) => {
      await issueInvoice(tx as unknown as LedgerClient, { bookId: seller.bookId, operationKey: `invoice:${operationKey}`, invoiceId, customerId: `person:${person}`, unit: 'USD', atoms: 1000n, occurredAt: new Date(), effectiveOn: '2028-01-01', chart, lines: [{ key: 'support:1', description: 'Support', atoms: 1000n }] });
      await tx.insert(schema.billingOrder).values({ id: orderId, operationKey, customerPersonId: person, sellerId: seller.id, offerNamespace: seller.id, offerId: 'support', offerVersion: 1, invoiceId, kind: 'one_time', state: 'invoice_open', totalAtoms: 1000n, paidAtoms: 0n });
    });
    const receipt = await database().transaction((tx) => recordPayment(tx as unknown as LedgerClient, { bookId: seller.bookId, operationKey: `receipt:${operationKey}`, paymentId: `manual:${orderId}`, provider: 'manual', economicKey: orderId, providerEventId: operationKey, customerId: `person:${person}`, invoiceId, unit: 'USD', atoms: 1250n, occurredAt: new Date(), effectiveOn: '2028-01-02', chart }));
    expect(receipt).toMatchObject({ appliedAtoms: 1000n, unappliedAtoms: 250n });
  });

  it('rolls a ledger invoice back with its enclosing app transaction', async () => {
    const seller = (await database().select().from(schema.billingSeller).where(eq(schema.billingSeller.id, 'isoastra')).limit(1))[0]!;
    const invoiceId = `rollback:${randomUUID()}`;
    await expect(database().transaction(async (tx) => {
      await issueInvoice(tx as unknown as LedgerClient, { bookId: seller.bookId, operationKey: invoiceId, invoiceId, customerId: 'rollback', unit: 'USD', atoms: 50n, occurredAt: new Date(), effectiveOn: '2028-01-01', chart: seller.chart as BillingChart, lines: [{ key: 'x', description: 'x', atoms: 50n }] });
      throw new Error('abort');
    })).rejects.toThrow('abort');
    const rows = await database().execute<{ n: string }>(`select count(*)::text n from billing_v2_invoice where id='${invoiceId}'`);
    expect(rows.rows[0]?.n).toBe('0');
  });
});
