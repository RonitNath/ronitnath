'use server';

import { randomUUID } from 'node:crypto';
import { and, eq, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { issueCreditNote, issueInvoice, recordPayment, type BillingChart } from '@isoastra/fleet-billing/accounting';
import { anchoredServiceEnd, invoiceOutstanding, membershipAccess, transitionManualBilling, validateOfferVersion, type FinancialAdjustmentEffect } from '@isoastra/fleet-billing/commerce';
import { savePriceVersion } from '@isoastra/fleet-billing/subscriptions';
import { postJournal, type LedgerClient } from '@isoastra/fleet-ledger';
import { publish, readDraft, saveDraft } from '@isoastra/fleet-configured';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { FormState } from '@/features/auth/form-state';
import { currentPrincipal } from '@/features/auth/principal';
import { decodeId, encodeId } from '@/lib/ids';
import { operatorPath, userPath } from '@/lib/paths';
import { requireOperator } from '@/lib/tiers';
import { emit } from '@/lib/fleet/events';
import { catalogSchema, dateOnly, materializeOffer } from './model';
import { beginCommand, completeCommand } from './operations';

const DECLINED = 'That billing command is unavailable or has changed.';
const keyOf = (seller: string) => `billing.catalog.${seller}`;
const field = (form: FormData, name: string) => typeof form.get(name) === 'string' ? String(form.get(name)) : '';
const expected = (form: FormData) => Number(field(form, 'version'));
const positiveAtoms = (raw: string) => /^\d+$/.test(raw) && BigInt(raw) > 0n ? BigInt(raw) : null;
const customerId = (personId: number) => encodeId('person', personId);
type AppChart = BillingChart & { deferredRevenue: string };
const ledger = (client: unknown) => client as LedgerClient;

async function member() {
  const principal = await currentPrincipal();
  if (!principal) redirect('/auth/sign-in');
  return principal;
}

async function sellerAndOffer(tx: Parameters<Parameters<ReturnType<typeof database>['transaction']>[0]>[0], sellerId: string, offerId: string) {
  const rows = await tx.select({ seller: schema.billingSeller, version: schema.billingOfferVersion })
    .from(schema.billingOffer).innerJoin(schema.billingOfferVersion, and(
      eq(schema.billingOffer.namespace, schema.billingOfferVersion.namespace),
      eq(schema.billingOffer.id, schema.billingOfferVersion.id),
      eq(schema.billingOffer.publishedVersion, schema.billingOfferVersion.version),
    )).innerJoin(schema.billingSeller, eq(schema.billingOfferVersion.sellerId, schema.billingSeller.id))
    .where(and(eq(schema.billingOffer.namespace, sellerId), eq(schema.billingOffer.id, offerId), eq(schema.billingOffer.available, true), eq(schema.billingSeller.enabled, true))).limit(1);
  const row = rows[0];
  if (!row) return null;
  return { seller: row.seller, offer: validateOfferVersion(materializeOffer(sellerId, row.version.version, catalogSchema.shape.offers.element.parse(row.version.terms))) };
}

async function sellerAndOfferVersion(tx: Parameters<Parameters<ReturnType<typeof database>['transaction']>[0]>[0], sellerId: string, offerId: string, version: number) {
  const row = (await tx.select({ seller: schema.billingSeller, version: schema.billingOfferVersion }).from(schema.billingOfferVersion)
    .innerJoin(schema.billingSeller, eq(schema.billingOfferVersion.sellerId, schema.billingSeller.id))
    .where(and(eq(schema.billingOfferVersion.namespace, sellerId), eq(schema.billingOfferVersion.id, offerId), eq(schema.billingOfferVersion.version, version))).limit(1))[0];
  return row ? { seller: row.seller, offer: validateOfferVersion(materializeOffer(sellerId, row.version.version, catalogSchema.shape.offers.element.parse(row.version.terms))) } : null;
}

const adjustmentEffect = (form: FormData): FinancialAdjustmentEffect => field(form, 'serviceEffect') === 'revoke_unfulfilled_service' ? 'revoke_unfulfilled_service' : 'preserve_service';

export async function saveCatalogDraft(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const sellerId = field(form, 'seller'), operationKey = field(form, 'operationKey'), version = expected(form);
  let body;
  try { body = catalogSchema.parse(JSON.parse(field(form, 'catalog'))); } catch { return { error: 'The catalog must be valid offer JSON.' }; }
  try {
    const savedVersion = await database().transaction(async (tx) => {
      const operation = await beginCommand(tx, { operationKey, actorPersonId: principal.personId, command: 'save-catalog-draft', request: { sellerId, version, body } });
      if (operation.retry) return Number((operation.result as { version: number }).version);
      const next = await saveDraft(tx, 'isoastra', keyOf(sellerId), body, version, customerId(principal.personId));
      await completeCommand(tx, operationKey, { version: next });
      return next;
    });
    revalidatePath(operatorPath('billing'));
    return { notice: `Draft ${savedVersion} saved.` };
  } catch { return { error: DECLINED }; }
}

export async function publishCatalog(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const sellerId = field(form, 'seller'), operationKey = field(form, 'operationKey'), expectedVersion = expected(form);
  try {
    const version = await database().transaction(async (tx) => {
      const operation = await beginCommand(tx, { operationKey, actorPersonId: principal.personId, command: 'publish-catalog', request: { sellerId, expectedVersion } });
      if (operation.retry) return Number((operation.result as { version: number }).version);
      const seller = (await tx.select().from(schema.billingSeller).where(eq(schema.billingSeller.id, sellerId)).limit(1))[0];
      if (!seller) throw new Error('seller');
      const draft = await readDraft(tx, 'isoastra', keyOf(sellerId));
      const body = catalogSchema.parse(draft.body);
      if (draft.version !== expectedVersion) throw new Error('stale');
      await publish(tx, 'isoastra', keyOf(sellerId), draft.version, customerId(principal.personId));
      for (const input of body.offers) {
        const offer = validateOfferVersion(materializeOffer(sellerId, draft.version, input));
        await tx.insert(schema.billingOfferVersion).values({ namespace: sellerId, id: offer.id, version: offer.version, sellerId, terms: input }).onConflictDoNothing();
        await tx.insert(schema.billingOffer).values({ namespace: sellerId, id: offer.id, publishedVersion: offer.version, available: offer.available })
          .onConflictDoUpdate({ target: [schema.billingOffer.namespace, schema.billingOffer.id], set: { publishedVersion: offer.version, available: offer.available, updatedAt: new Date() } });
        if (offer.kind === 'subscription') await savePriceVersion(ledger(tx), { id: offer.id, namespace: sellerId, version: offer.version, currency: 'USD', interval: offer.interval!, model: { kind: 'flat', amountAtoms: offer.amountAtoms }, recognition: 'advance_deferred' });
      }
      await completeCommand(tx, operationKey, { version: draft.version });
      return draft.version;
    });
    revalidatePath(operatorPath('billing'));
    return { notice: `Catalog version ${version} published.` };
  } catch { return { error: DECLINED }; }
}

export async function savePilotOfferDraft(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const sellerId = field(form, 'seller'), operationKey = field(form, 'operationKey'), version = expected(form);
  const amountAtoms = field(form, 'amountAtoms');
  const paymentDueDays = Number(field(form, 'paymentDueDays'));
  const body = catalogSchema.safeParse({ offers: [{
    id: field(form, 'offerId'), title: field(form, 'title'), description: field(form, 'description'),
    kind: 'one_time', amountAtoms, interval: null, accessPolicy: 'payment_first', trialDays: 0,
    paymentDueDays, graceDays: 0, fulfillment: field(form, 'fulfillment'), benefits: {}, available: field(form, 'available') === 'true',
  }] });
  if (!body.success) return { error: 'Enter valid one-time offer terms and a nonnegative USD-cent price.' };
  try {
    const savedVersion = await database().transaction(async (tx) => {
      const operation = await beginCommand(tx, { operationKey, actorPersonId: principal.personId, command: 'save-pilot-offer-draft', request: { sellerId, version, body: body.data } });
      if (operation.retry) return Number((operation.result as { version: number }).version);
      const next = await saveDraft(tx, 'isoastra', keyOf(sellerId), body.data, version, customerId(principal.personId));
      await completeCommand(tx, operationKey, { version: next });
      return next;
    });
    revalidatePath(operatorPath('billing'));
    return { notice: `Pilot offer draft ${savedVersion} saved. Review it before publication.` };
  } catch { return { error: DECLINED }; }
}

export async function addPilotAccount(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const operationKey = field(form, 'operationKey');
  let personId: number;
  try { personId = decodeId('person', field(form, 'person')); } catch { return { error: 'Enter a valid public person ID.' }; }
  try { await database().transaction(async (tx) => {
    const operation = await beginCommand(tx, { operationKey, actorPersonId: principal.personId, command: 'add-pilot-account', request: { personId } });
    if (operation.retry) return;
    const exists = await tx.select({ id: schema.person.id }).from(schema.person).where(eq(schema.person.id, personId)).limit(1);
    if (!exists.length) throw new Error('person');
    await tx.insert(schema.billingPilotAccount).values({ personId }).onConflictDoUpdate({ target: schema.billingPilotAccount.personId, set: { enabled: true, version: sql`${schema.billingPilotAccount.version}+1`, updatedAt: new Date() } });
    await completeCommand(tx, operationKey, { personId });
  }); } catch { return { error: DECLINED }; }
  revalidatePath(operatorPath('billing'));
  return { notice: 'Pilot account enabled.' };
}

export async function setPilotAccountEnabled(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const personId = Number(field(form, 'person')), version = expected(form), enabled = field(form, 'enabled') === 'true', operationKey = field(form, 'operationKey');
  try { await database().transaction(async (tx) => {
    const operation = await beginCommand(tx, { operationKey, actorPersonId: principal.personId, command: 'set-pilot-account-enabled', request: { personId, version, enabled } });
    if (operation.retry) return;
    const rows = await tx.update(schema.billingPilotAccount).set({ enabled, version: version + 1, updatedAt: new Date() }).where(and(eq(schema.billingPilotAccount.personId, personId), eq(schema.billingPilotAccount.version, version))).returning({ id: schema.billingPilotAccount.personId });
    if (!rows.length) throw new Error('stale');
    await completeCommand(tx, operationKey, { personId, version: version + 1, enabled });
  }); } catch { return { error: DECLINED }; }
  revalidatePath(operatorPath('billing'));
  return { notice: enabled ? 'Pilot account enabled.' : 'New purchases disabled for this account.' };
}

export async function setSellerEnabled(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const sellerId = field(form, 'seller');
  const enabled = field(form, 'enabled') === 'true', version = expected(form), operationKey = field(form, 'operationKey');
  const changed = await database().transaction(async (tx) => {
    const operation = await beginCommand(tx, { operationKey, actorPersonId: principal.personId, command: 'set-seller-enabled', request: { sellerId, version, enabled } });
    if (operation.retry) return [operation.result];
    const seller = (await tx.select().from(schema.billingSeller).where(and(eq(schema.billingSeller.id, sellerId), eq(schema.billingSeller.version, version))).limit(1))[0];
    if (!seller) return [];
    if (enabled) {
      const period = await tx.execute(sql`SELECT 1 FROM fleet_ledger_period WHERE book_id=${seller.bookId} AND state='open' AND starts_on <= current_date AND ends_on > current_date LIMIT 1`);
      if (!period.rowCount) return [];
    }
    const rows = await tx.update(schema.billingSeller).set({ enabled, version: version + 1 }).where(and(eq(schema.billingSeller.id, sellerId), eq(schema.billingSeller.version, version))).returning({ id: schema.billingSeller.id });
    if (rows.length) await completeCommand(tx, operationKey, { sellerId, version: version + 1, enabled });
    return rows;
  });
  if (!changed.length) return { error: DECLINED };
  revalidatePath(operatorPath('billing'));
  return { notice: `${sellerId} ${enabled ? 'enabled' : 'disabled'}.` };
}

export async function requestInvoice(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await member();
  const sellerId = field(form, 'seller'), offerId = field(form, 'offer'), offerVersion = expected(form), operationKey = field(form, 'operationKey');
  if (!operationKey || !Number.isSafeInteger(offerVersion)) return { error: DECLINED };
  try {
    await database().transaction(async (tx) => {
      const retry = (await tx.select().from(schema.billingOrder).where(eq(schema.billingOrder.operationKey, operationKey)).limit(1))[0];
      if (retry) {
        if (retry.customerPersonId !== me.personId || retry.sellerId !== sellerId || retry.offerId !== offerId || retry.offerVersion !== offerVersion) throw new Error('changed retry');
        return;
      }
      const eligible = await tx.select().from(schema.billingPilotAccount).where(and(eq(schema.billingPilotAccount.personId, me.personId), eq(schema.billingPilotAccount.enabled, true))).limit(1);
      if (!eligible.length) throw new Error('pilot');
      const found = await sellerAndOffer(tx, sellerId, offerId);
      if (!found || found.offer.kind !== 'one_time' || found.offer.version !== offerVersion) throw new Error('offer');
      const { seller, offer } = found, now = new Date(), orderId = randomUUID(), invoiceId = `rn:${orderId}`;
      const chart = seller.chart as AppChart;
      await issueInvoice(ledger(tx), { bookId: seller.bookId, operationKey: `invoice:${operationKey}`, invoiceId, customerId: customerId(me.personId), unit: 'USD', atoms: offer.amountAtoms, occurredAt: now, effectiveOn: dateOnly(now), chart: { ...chart, revenue: chart.deferredRevenue }, lines: [{ key: `${offer.namespace}:${offer.id}:${offer.version}`, description: offer.title, atoms: offer.amountAtoms }] });
      const fulfilled = offer.kind === 'one_time' && offer.fulfillment === 'immediate';
      await tx.insert(schema.billingOrder).values({ id: orderId, operationKey, customerPersonId: me.personId, sellerId, offerNamespace: offer.namespace, offerId: offer.id, offerVersion: offer.version, invoiceId, kind: offer.kind, state: fulfilled ? 'fulfilled' : 'invoice_open', totalAtoms: offer.amountAtoms, paidAtoms: 0n, fulfilledAt: fulfilled ? now : null });
      if (fulfilled) await recognize(tx, seller.bookId, chart, offer.amountAtoms, `fulfill:${operationKey}`, `order:${orderId}`, now);
      await recordAudit(tx, { actorPersonId: me.personId, command: 'request-invoice', targetKind: 'person', targetId: me.personId, payload: { orderId, invoiceId, sellerId, offerId, offerVersion: offer.version } });
      await emit(tx, { orgId: customerId(me.personId), resourceKind: 'billing-order', resourceId: orderId, kind: 'created' });
    });
    revalidatePath(userPath(customerId(me.personId), 'billing'));
    return { notice: 'Invoice requested.' };
  } catch { return { error: DECLINED }; }
}

async function recognize(tx: Parameters<Parameters<ReturnType<typeof database>['transaction']>[0]>[0], bookId: string, chart: AppChart, atoms: bigint, operationKey: string, evidenceId: string, now: Date) {
  if (atoms === 0n) return;
  await postJournal(ledger(tx), { bookId, operationKey, kind: 'billing.revenue.recognition', occurredAt: now, effectiveOn: dateOnly(now), evidence: [{ kind: 'fulfillment', id: evidenceId }], policy: { key: 'billing.fulfillment', version: 1 }, postings: [{ accountId: chart.deferredRevenue, unit: 'USD', atoms }, { accountId: chart.revenue, unit: 'USD', atoms: -atoms }] });
}

export async function claimPayment(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await member();
  const atoms = positiveAtoms(field(form, 'amountAtoms')), operationKey = field(form, 'operationKey'), invoiceId = field(form, 'invoice'), evidence = field(form, 'evidence').trim(), orderVersion = expected(form);
  if (!atoms || !operationKey || evidence.length < 3 || evidence.length > 2000) return { error: 'Enter a positive amount and identifying payment evidence.' };
  try {
    await database().transaction(async (tx) => {
      const retry = (await tx.select().from(schema.billingReceiptClaim).where(eq(schema.billingReceiptClaim.operationKey, operationKey)).limit(1))[0];
      if (retry) {
        if (retry.customerPersonId !== me.personId || retry.invoiceId !== invoiceId || retry.amountAtoms !== atoms || retry.evidence !== evidence) throw new Error('changed retry');
        return;
      }
      const order = (await tx.select().from(schema.billingOrder).where(and(eq(schema.billingOrder.invoiceId, invoiceId), eq(schema.billingOrder.customerPersonId, me.personId), eq(schema.billingOrder.version, orderVersion))).limit(1))[0];
      if (!order) throw new Error('invoice');
      await tx.insert(schema.billingReceiptClaim).values({ operationKey, customerPersonId: me.personId, sellerId: order.sellerId, invoiceId, amountAtoms: atoms, evidence });
      await emit(tx, { orgId: 'isoastra', resourceKind: 'billing-receipt', resourceId: operationKey, kind: 'claimed' });
    });
    revalidatePath(userPath(customerId(me.personId), 'billing'));
    return { notice: 'Payment evidence submitted for confirmation.' };
  } catch { return { error: DECLINED }; }
}

export async function confirmReceipt(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const claimId = field(form, 'claim'), operationKey = field(form, 'operationKey'), claimVersion = Number(field(form, 'claimVersion')), orderVersion = expected(form);
  const externalNamespace = field(form, 'externalNamespace').trim(), externalId = field(form, 'externalId').trim();
  if (!operationKey || !externalNamespace || !externalId) return { error: 'Enter the source namespace and external receipt ID.' };
  try {
    await database().transaction(async (tx) => {
      const row = (await tx.select({ claim: schema.billingReceiptClaim, order: schema.billingOrder, seller: schema.billingSeller, terms: schema.billingOfferVersion.terms })
        .from(schema.billingReceiptClaim).innerJoin(schema.billingOrder, eq(schema.billingReceiptClaim.invoiceId, schema.billingOrder.invoiceId))
        .innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id))
        .innerJoin(schema.billingOfferVersion, and(eq(schema.billingOrder.offerNamespace, schema.billingOfferVersion.namespace), eq(schema.billingOrder.offerId, schema.billingOfferVersion.id), eq(schema.billingOrder.offerVersion, schema.billingOfferVersion.version)))
        .where(eq(schema.billingReceiptClaim.id, claimId)).limit(1))[0];
      if (!row) throw new Error('claim');
      if (row.claim.state === 'confirmed') {
        if (row.claim.confirmedOperationKey !== operationKey || row.claim.confirmedExternalNamespace !== externalNamespace || row.claim.confirmedExternalId !== externalId) throw new Error('changed retry');
        return;
      }
      if (row.claim.state !== 'pending' || row.claim.version !== claimVersion || row.order.version !== orderVersion) throw new Error('stale');
      const now = new Date(), chart = row.seller.chart as AppChart;
      const transition = transitionManualBilling({ version: row.order.version, totalAtoms: row.order.totalAtoms, paidAtoms: row.order.paidAtoms, creditedAtoms: row.order.creditedAtoms, refundedAtoms: row.order.refundedAtoms, fulfilled: !!row.order.fulfilledAt }, { kind: 'confirm_receipt', expectedVersion: orderVersion, amountAtoms: row.claim.amountAtoms }, 'operator');
      const receipt = await recordPayment(ledger(tx), { bookId: row.seller.bookId, operationKey, paymentId: `manual:${claimId}`, provider: 'manual', economicKey: claimId, providerEventId: operationKey, customerId: customerId(row.order.customerPersonId), invoiceId: row.order.invoiceId, unit: 'USD', atoms: row.claim.amountAtoms, occurredAt: now, effectiveOn: dateOnly(now), chart });
      if (receipt.appliedAtoms !== -transition.effects.receivableAtoms || receipt.unappliedAtoms !== transition.effects.customerCreditAtoms) throw new Error('transition mismatch');
      const claimRows = await tx.update(schema.billingReceiptClaim).set({ state: 'confirmed', version: claimVersion + 1, confirmedOperationKey: operationKey, confirmedExternalNamespace: externalNamespace, confirmedExternalId: externalId, appliedAtoms: receipt.appliedAtoms, unappliedAtoms: receipt.unappliedAtoms, confirmedAt: now }).where(and(eq(schema.billingReceiptClaim.id, claimId), eq(schema.billingReceiptClaim.state, 'pending'), eq(schema.billingReceiptClaim.version, claimVersion))).returning({ id: schema.billingReceiptClaim.id });
      const outstanding = invoiceOutstanding(transition.state);
      const orderRows = await tx.update(schema.billingOrder).set({ paidAtoms: transition.state.paidAtoms, state: outstanding === 0n ? 'paid' : 'partially_paid', version: transition.state.version }).where(and(eq(schema.billingOrder.id, row.order.id), eq(schema.billingOrder.version, orderVersion))).returning({ id: schema.billingOrder.id });
      if (!claimRows.length || !orderRows.length) throw new Error('stale');
      if (outstanding === 0n && row.order.kind === 'subscription') {
        const offer = materializeOffer(row.order.sellerId, row.order.offerVersion, catalogSchema.shape.offers.element.parse(row.terms));
        const membership = (await tx.select().from(schema.billingMembership).where(eq(schema.billingMembership.orderId, row.order.id)).limit(1))[0];
        if (membership && offer.accessPolicy === 'payment_first' && offer.trialDays === 0 && row.order.renewsOrderId === null) {
          const end = anchoredServiceEnd({ startsAt: now, interval: offer.interval!, timeZone: membership.timeZone, anchorDay: membership.anchorDay, anchorTime: membership.anchorTime });
          await tx.update(schema.billingMembership).set({ state: 'active', serviceStartsAt: now, serviceEndsAt: end, version: sql`${schema.billingMembership.version}+1` }).where(eq(schema.billingMembership.id, membership.id));
        }
      }
      await recordAudit(tx, { actorPersonId: principal.personId, command: 'confirm-receipt', targetKind: 'person', targetId: row.order.customerPersonId, payload: { claimId, orderId: row.order.id, appliedAtoms: receipt.appliedAtoms.toString(), unappliedAtoms: receipt.unappliedAtoms.toString() } });
      await emit(tx, { orgId: customerId(row.order.customerPersonId), resourceKind: 'billing-receipt', resourceId: claimId, kind: 'confirmed' });
    });
    revalidatePath(operatorPath('billing'));
    return { notice: 'Receipt confirmed and allocated.' };
  } catch (error) {
    console.error('billing command failed', { command: 'confirm-receipt', message: error instanceof Error ? error.message : 'unknown' });
    return { error: DECLINED };
  }
}

export async function fulfillOrder(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const orderId = field(form, 'order'), version = expected(form), operationKey = field(form, 'operationKey');
  try {
    await database().transaction(async (tx) => {
      const row = (await tx.select({ order: schema.billingOrder, seller: schema.billingSeller }).from(schema.billingOrder).innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id)).where(eq(schema.billingOrder.id, orderId)).limit(1))[0];
      if (!row || row.order.kind !== 'one_time') throw new Error('order');
      if (row.order.fulfilledAt) { if (row.order.fulfilledOperationKey === operationKey) return; throw new Error('fulfilled'); }
      const revoked = await tx.select({ id: schema.billingAdjustment.id }).from(schema.billingAdjustment).where(and(eq(schema.billingAdjustment.orderId, orderId), eq(schema.billingAdjustment.serviceEffect, 'revoke_unfulfilled_service'))).limit(1);
      if (revoked.length) throw new Error('service revoked');
      const transition = transitionManualBilling({ version: row.order.version, totalAtoms: row.order.totalAtoms, paidAtoms: row.order.paidAtoms, creditedAtoms: row.order.creditedAtoms, refundedAtoms: row.order.refundedAtoms, fulfilled: false }, { kind: 'fulfill', expectedVersion: version }, 'operator');
      const now = new Date();
      await recognize(tx, row.seller.bookId, row.seller.chart as AppChart, transition.effects.recognizeAtoms, operationKey, `order:${orderId}`, now);
      const changed = await tx.update(schema.billingOrder).set({ fulfilledAt: now, fulfilledOperationKey: operationKey, state: 'fulfilled', version: transition.state.version }).where(and(eq(schema.billingOrder.id, orderId), eq(schema.billingOrder.version, version))).returning({ id: schema.billingOrder.id });
      if (!changed.length) throw new Error('stale');
      await recordAudit(tx, { actorPersonId: principal.personId, command: 'fulfill-order', targetKind: 'person', targetId: row.order.customerPersonId, payload: { orderId } });
    });
    revalidatePath(operatorPath('billing'));
    return { notice: 'Fulfillment recorded.' };
  } catch { return { error: DECLINED }; }
}

export async function changeMembership(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await member();
  const membershipId = field(form, 'membership'), offerId = field(form, 'offer'), version = expected(form), operationKey = field(form, 'operationKey');
  try {
    const changed = await database().transaction(async (tx) => {
      const operation = await beginCommand(tx, { operationKey, actorPersonId: me.personId, command: 'change-membership', request: { membershipId, offerId, version } });
      if (operation.retry) return true;
      const row = (await tx.select({ membership: schema.billingMembership, order: schema.billingOrder }).from(schema.billingMembership).innerJoin(schema.billingOrder, eq(schema.billingMembership.orderId, schema.billingOrder.id)).where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version), eq(schema.billingOrder.customerPersonId, me.personId))).limit(1))[0];
      if (!row) return false;
      const target = await sellerAndOffer(tx, row.order.sellerId, offerId);
      if (!target || target.offer.kind !== 'subscription') return false;
      const rows = await tx.update(schema.billingMembership).set({ pendingOfferId: target.offer.id, pendingOfferVersion: target.offer.version, version: version + 1 }).where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version))).returning({ id: schema.billingMembership.id });
      if (!rows.length) throw new Error('stale');
      await completeCommand(tx, operationKey, { membershipId, version: version + 1 });
      return true;
    });
    if (!changed) return { error: DECLINED };
    revalidatePath(userPath(customerId(me.personId), 'billing'));
    return { notice: 'Tier change scheduled for the next cycle.' };
  } catch { return { error: DECLINED }; }
}

export async function cancelRenewal(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await member(), membershipId = field(form, 'membership'), version = expected(form);
  const operationKey = field(form, 'operationKey');
  try { await database().transaction(async (tx) => {
    const operation = await beginCommand(tx, { operationKey, actorPersonId: me.personId, command: 'cancel-renewal', request: { membershipId, version } });
    if (operation.retry) return;
    const rows = await tx.update(schema.billingMembership).set({ cancelAt: sql`${schema.billingMembership.serviceEndsAt}`, version: version + 1 }).from(schema.billingOrder)
      .where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version), eq(schema.billingMembership.orderId, schema.billingOrder.id), eq(schema.billingOrder.customerPersonId, me.personId))).returning({ id: schema.billingMembership.id });
    if (!rows.length) throw new Error('stale');
    await completeCommand(tx, operationKey, { membershipId, version: version + 1 });
  }); } catch { return { error: DECLINED }; }
  revalidatePath(userPath(customerId(me.personId), 'billing'));
  return { notice: 'Renewal cancelled. Existing balances remain due.' };
}

export async function setMembershipSuspended(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const membershipId = field(form, 'membership'), version = expected(form), suspend = field(form, 'suspend') === 'true', operationKey = field(form, 'operationKey');
  try { await database().transaction(async (tx) => {
    const operation = await beginCommand(tx, { operationKey, actorPersonId: principal.personId, command: 'set-membership-suspended', request: { membershipId, version, suspend } });
    if (operation.retry) return;
    const rows = await tx.update(schema.billingMembership).set({ suspendedAt: suspend ? new Date() : null, state: suspend ? 'suspended' : 'active', version: version + 1 })
      .where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version))).returning({ id: schema.billingMembership.id });
    if (!rows.length) throw new Error('stale');
    await completeCommand(tx, operationKey, { membershipId, version: version + 1, suspended: suspend });
  }); } catch { return { error: DECLINED }; }
  revalidatePath(operatorPath('billing'));
  return { notice: suspend ? 'Membership suspended.' : 'Membership resumed.' };
}

export async function renewMembership(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const membershipId = field(form, 'membership'), version = expected(form), operationKey = field(form, 'operationKey');
  try {
    await database().transaction(async (tx) => {
      const operation = await beginCommand(tx, { operationKey, actorPersonId: principal.personId, command: 'renew-membership', request: { membershipId, version } });
      if (operation.retry) return;
      const row = (await tx.select({ membership: schema.billingMembership, order: schema.billingOrder, seller: schema.billingSeller })
        .from(schema.billingMembership).innerJoin(schema.billingOrder, eq(schema.billingMembership.orderId, schema.billingOrder.id)).innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id))
        .where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version))).limit(1))[0];
      const now = new Date();
      if (!row || now < row.membership.serviceEndsAt || row.membership.suspendedAt) throw new Error('membership');
      if (row.membership.cancelAt) {
        const rows = await tx.update(schema.billingMembership).set({ state: 'cancelled', version: version + 1 }).where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version))).returning({ id: schema.billingMembership.id });
        if (!rows.length) throw new Error('stale');
        await completeCommand(tx, operationKey, { membershipId, version: version + 1, cancelled: true });
        return;
      }
      const nextOfferId = row.membership.pendingOfferId ?? row.order.offerId;
      const nextOfferVersion = row.membership.pendingOfferVersion ?? row.order.offerVersion;
      const found = await sellerAndOfferVersion(tx, row.order.sellerId, nextOfferId, nextOfferVersion);
      if (!found || found.offer.kind !== 'subscription') throw new Error('offer');
      const { offer, seller } = found, startsAt = row.membership.serviceEndsAt;
      const endsAt = anchoredServiceEnd({ startsAt, interval: offer.interval!, timeZone: row.membership.timeZone, anchorDay: row.membership.anchorDay, anchorTime: row.membership.anchorTime });
      const orderId = randomUUID(), invoiceId = `rn:${orderId}`, chart = seller.chart as AppChart;
      await issueInvoice(ledger(tx), { bookId: seller.bookId, operationKey: `invoice:${operationKey}`, invoiceId, customerId: customerId(row.order.customerPersonId), unit: 'USD', atoms: offer.amountAtoms, occurredAt: now, effectiveOn: dateOnly(now), chart: { ...chart, revenue: chart.deferredRevenue }, lines: [{ key: `${offer.namespace}:${offer.id}:${offer.version}`, description: `${offer.title} renewal`, atoms: offer.amountAtoms }] });
      await recognize(tx, seller.bookId, chart, row.order.totalAtoms, `recognize:${operationKey}`, `order:${row.order.id}`, now);
      await tx.insert(schema.billingOrder).values({ id: orderId, operationKey, customerPersonId: row.order.customerPersonId, sellerId: seller.id, offerNamespace: offer.namespace, offerId: offer.id, offerVersion: offer.version, invoiceId, kind: 'subscription', state: offer.accessPolicy === 'invoice_first' ? 'invoice_open' : 'invoice_open', totalAtoms: offer.amountAtoms, paidAtoms: 0n, renewsOrderId: row.order.id });
      const rows = await tx.update(schema.billingMembership).set({ orderId, state: offer.accessPolicy === 'invoice_first' ? 'active' : 'pending', serviceStartsAt: startsAt, serviceEndsAt: endsAt, pendingOfferId: null, pendingOfferVersion: null, version: version + 1 }).where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version))).returning({ id: schema.billingMembership.id });
      if (!rows.length) throw new Error('stale');
      await completeCommand(tx, operationKey, { membershipId, version: version + 1, orderId });
    });
    revalidatePath(operatorPath('billing'));
    return { notice: 'Membership advanced to its next anchored interval.' };
  } catch { return { error: DECLINED }; }
}

export async function membershipStatus(membershipId: string, now = new Date()) {
  const row = (await database().select({ membership: schema.billingMembership, order: schema.billingOrder, terms: schema.billingOfferVersion.terms }).from(schema.billingMembership)
    .innerJoin(schema.billingOrder, eq(schema.billingMembership.orderId, schema.billingOrder.id)).innerJoin(schema.billingOfferVersion, and(eq(schema.billingOrder.offerNamespace, schema.billingOfferVersion.namespace), eq(schema.billingOrder.offerId, schema.billingOfferVersion.id), eq(schema.billingOrder.offerVersion, schema.billingOfferVersion.version))).where(eq(schema.billingMembership.id, membershipId)).limit(1))[0];
  if (!row) return null;
  return membershipAccess({ offer: materializeOffer(row.order.sellerId, row.order.offerVersion, catalogSchema.shape.offers.element.parse(row.terms)), acceptedAt: row.order.createdAt, serviceStartsAt: row.membership.serviceStartsAt, serviceEndsAt: row.membership.serviceEndsAt, paidAtoms: row.order.paidAtoms, suspended: !!row.membership.suspendedAt, cancelled: !!row.membership.cancelAt && now >= row.membership.cancelAt, now });
}

export async function creditOrder(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const orderId = field(form, 'order'), operationKey = field(form, 'operationKey'), atoms = positiveAtoms(field(form, 'amountAtoms')), version = expected(form), effect = adjustmentEffect(form), evidence = field(form, 'evidence');
  if (!atoms) return { error: DECLINED };
  try {
    await database().transaction(async (tx) => {
      const retry = (await tx.select().from(schema.billingAdjustment).where(eq(schema.billingAdjustment.operationKey, operationKey)).limit(1))[0];
      if (retry) { if (retry.orderId !== orderId || retry.kind !== 'credit' || retry.amountAtoms !== atoms || retry.serviceEffect !== effect || retry.evidence !== evidence) throw new Error('changed retry'); return; }
      const row = (await tx.select({ order: schema.billingOrder, seller: schema.billingSeller }).from(schema.billingOrder).innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id)).where(eq(schema.billingOrder.id, orderId)).limit(1))[0];
      if (!row) throw new Error('order');
      const transition = transitionManualBilling({ version: row.order.version, totalAtoms: row.order.totalAtoms, paidAtoms: row.order.paidAtoms, creditedAtoms: row.order.creditedAtoms, refundedAtoms: row.order.refundedAtoms, fulfilled: !!row.order.fulfilledAt }, { kind: 'credit', expectedVersion: version, amountAtoms: atoms, effect }, 'operator');
      const now = new Date();
      const chart = row.seller.chart as AppChart;
      await issueCreditNote(ledger(tx), { creditNoteId: `credit:${operationKey}`, invoiceId: row.order.invoiceId, bookId: row.seller.bookId, operationKey, unit: 'USD', atoms, occurredAt: now, effectiveOn: dateOnly(now), chart: { ...chart, contraRevenue: row.order.fulfilledAt ? chart.contraRevenue : chart.deferredRevenue } });
      await tx.insert(schema.billingAdjustment).values({ operationKey, orderId, kind: 'credit', amountAtoms: atoms, evidence, serviceEffect: effect });
      const nextState = effect === 'revoke_unfulfilled_service' ? 'service_revoked' : invoiceOutstanding(transition.state) === 0n ? 'credited' : row.order.state;
      const changed = await tx.update(schema.billingOrder).set({ creditedAtoms: transition.state.creditedAtoms, state: nextState, version: transition.state.version }).where(and(eq(schema.billingOrder.id, orderId), eq(schema.billingOrder.version, version))).returning({ id: schema.billingOrder.id });
      if (!changed.length) throw new Error('stale');
      await recordAudit(tx, { actorPersonId: principal.personId, command: 'credit-order', targetKind: 'person', targetId: row.order.customerPersonId, payload: { orderId, atoms: atoms.toString() } });
    });
    revalidatePath(operatorPath('billing'));
    return { notice: 'Credit note recorded.' };
  } catch { return { error: DECLINED }; }
}

export async function recordExternalRefund(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const orderId = field(form, 'order'), operationKey = field(form, 'operationKey'), atoms = positiveAtoms(field(form, 'amountAtoms')), evidence = field(form, 'evidence').trim(), externalNamespace = field(form, 'externalNamespace').trim(), externalId = field(form, 'externalId').trim(), version = expected(form), effect = adjustmentEffect(form);
  if (!atoms || evidence.length < 3 || !externalNamespace || !externalId) return { error: DECLINED };
  try {
    await database().transaction(async (tx) => {
      const retry = (await tx.select().from(schema.billingAdjustment).where(eq(schema.billingAdjustment.operationKey, operationKey)).limit(1))[0];
      if (retry) { if (retry.orderId !== orderId || retry.kind !== 'refund' || retry.amountAtoms !== atoms || retry.serviceEffect !== effect || retry.externalNamespace !== externalNamespace || retry.externalId !== externalId) throw new Error('changed retry'); return; }
      const row = (await tx.select({ order: schema.billingOrder, seller: schema.billingSeller }).from(schema.billingOrder).innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id)).where(eq(schema.billingOrder.id, orderId)).limit(1))[0];
      if (!row) throw new Error('refund');
      const transition = transitionManualBilling({ version: row.order.version, totalAtoms: row.order.totalAtoms, paidAtoms: row.order.paidAtoms, creditedAtoms: row.order.creditedAtoms, refundedAtoms: row.order.refundedAtoms, fulfilled: !!row.order.fulfilledAt }, { kind: 'refund', expectedVersion: version, amountAtoms: atoms, effect }, 'operator');
      const chart = row.seller.chart as AppChart, now = new Date();
      await postJournal(ledger(tx), { bookId: row.seller.bookId, operationKey, kind: 'billing.external_refund', occurredAt: now, effectiveOn: dateOnly(now), evidence: [{ kind: externalNamespace, id: externalId }], policy: { key: 'billing.external-refund', version: 2 }, postings: [{ accountId: row.order.fulfilledAt ? chart.contraRevenue : chart.deferredRevenue, unit: 'USD', atoms }, { accountId: chart.processorClearing, unit: 'USD', atoms: -atoms }] });
      await tx.insert(schema.billingAdjustment).values({ operationKey, orderId, kind: 'refund', amountAtoms: atoms, evidence, serviceEffect: effect, externalNamespace, externalId });
      const changed = await tx.update(schema.billingOrder).set({ refundedAtoms: transition.state.refundedAtoms, state: effect === 'revoke_unfulfilled_service' ? 'service_revoked' : row.order.state, version: transition.state.version }).where(and(eq(schema.billingOrder.id, orderId), eq(schema.billingOrder.version, version))).returning({ id: schema.billingOrder.id });
      if (!changed.length) throw new Error('stale');
      await recordAudit(tx, { actorPersonId: principal.personId, command: 'record-external-refund', targetKind: 'person', targetId: row.order.customerPersonId, payload: { orderId, atoms: atoms.toString(), externalNamespace, externalId, serviceEffect: effect } });
    });
    revalidatePath(operatorPath('billing'));
    return { notice: 'Externally completed refund recorded.' };
  } catch { return { error: DECLINED }; }
}
