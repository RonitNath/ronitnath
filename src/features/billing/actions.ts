'use server';

import { randomUUID } from 'node:crypto';
import { and, eq, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { issueCreditNote, issueInvoice, recordPayment, type BillingChart } from '@isoastra/fleet-billing/accounting';
import { anchoredServiceEnd, membershipAccess, validateOfferVersion } from '@isoastra/fleet-billing/commerce';
import { savePriceVersion } from '@isoastra/fleet-billing/subscriptions';
import { postJournal, type LedgerClient } from '@isoastra/fleet-ledger';
import { publish, readDraft, saveDraft } from '@isoastra/fleet-configured';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { FormState } from '@/features/auth/form-state';
import { currentPrincipal } from '@/features/auth/principal';
import { encodeId } from '@/lib/ids';
import { operatorPath, userPath } from '@/lib/paths';
import { requireOperator } from '@/lib/tiers';
import { emit } from '@/lib/fleet/events';
import { catalogSchema, dateOnly, materializeOffer } from './model';

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

export async function saveCatalogDraft(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const sellerId = field(form, 'seller');
  let body;
  try { body = catalogSchema.parse(JSON.parse(field(form, 'catalog'))); } catch { return { error: 'The catalog must be valid offer JSON.' }; }
  try {
    const version = await database().transaction((tx) => saveDraft(tx, 'isoastra', keyOf(sellerId), body, expected(form), customerId(principal.personId)));
    revalidatePath(operatorPath('billing'));
    return { notice: `Draft ${version} saved.` };
  } catch { return { error: DECLINED }; }
}

export async function publishCatalog(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const sellerId = field(form, 'seller');
  try {
    const version = await database().transaction(async (tx) => {
      const seller = (await tx.select().from(schema.billingSeller).where(eq(schema.billingSeller.id, sellerId)).limit(1))[0];
      if (!seller) throw new Error('seller');
      const draft = await readDraft(tx, 'isoastra', keyOf(sellerId));
      const body = catalogSchema.parse(draft.body);
      if (draft.version !== expected(form)) throw new Error('stale');
      await publish(tx, 'isoastra', keyOf(sellerId), draft.version, customerId(principal.personId));
      for (const input of body.offers) {
        const offer = validateOfferVersion(materializeOffer(sellerId, draft.version, input));
        await tx.insert(schema.billingOfferVersion).values({ namespace: sellerId, id: offer.id, version: offer.version, sellerId, terms: input }).onConflictDoNothing();
        await tx.insert(schema.billingOffer).values({ namespace: sellerId, id: offer.id, publishedVersion: offer.version, available: offer.available })
          .onConflictDoUpdate({ target: [schema.billingOffer.namespace, schema.billingOffer.id], set: { publishedVersion: offer.version, available: offer.available, updatedAt: new Date() } });
        if (offer.kind === 'subscription') await savePriceVersion(ledger(tx), { id: offer.id, namespace: sellerId, version: offer.version, currency: 'USD', interval: offer.interval!, model: { kind: 'flat', amountAtoms: offer.amountAtoms }, recognition: 'advance_deferred' });
      }
      return draft.version;
    });
    revalidatePath(operatorPath('billing'));
    return { notice: `Catalog version ${version} published.` };
  } catch { return { error: DECLINED }; }
}

export async function setSellerEnabled(_prev: FormState, form: FormData): Promise<FormState> {
  await requireOperator();
  const sellerId = field(form, 'seller');
  const enabled = field(form, 'enabled') === 'true';
  const changed = await database().update(schema.billingSeller).set({ enabled }).where(eq(schema.billingSeller.id, sellerId)).returning({ id: schema.billingSeller.id });
  if (!changed.length) return { error: DECLINED };
  revalidatePath(operatorPath('billing'));
  return { notice: `${sellerId} ${enabled ? 'enabled' : 'disabled'}.` };
}

export async function requestInvoice(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await member();
  const sellerId = field(form, 'seller'), offerId = field(form, 'offer'), operationKey = field(form, 'operationKey');
  if (!operationKey) return { error: DECLINED };
  try {
    await database().transaction(async (tx) => {
      const found = await sellerAndOffer(tx, sellerId, offerId);
      if (!found) throw new Error('offer');
      const { seller, offer } = found, now = new Date(), orderId = randomUUID(), invoiceId = `rn:${orderId}`;
      const chart = seller.chart as AppChart;
      await issueInvoice(ledger(tx), { bookId: seller.bookId, operationKey: `invoice:${operationKey}`, invoiceId, customerId: customerId(me.personId), unit: 'USD', atoms: offer.amountAtoms, occurredAt: now, effectiveOn: dateOnly(now), chart: { ...chart, revenue: chart.deferredRevenue }, lines: [{ key: `${offer.namespace}:${offer.id}:${offer.version}`, description: offer.title, atoms: offer.amountAtoms }] });
      const fulfilled = offer.kind === 'one_time' && offer.fulfillment === 'immediate';
      await tx.insert(schema.billingOrder).values({ id: orderId, operationKey, customerPersonId: me.personId, sellerId, offerNamespace: offer.namespace, offerId: offer.id, offerVersion: offer.version, invoiceId, kind: offer.kind, state: fulfilled ? 'fulfilled' : 'invoice_open', totalAtoms: offer.amountAtoms, paidAtoms: 0n, fulfilledAt: fulfilled ? now : null });
      if (fulfilled) await recognize(tx, seller.bookId, chart, offer.amountAtoms, `fulfill:${operationKey}`, `order:${orderId}`, now);
      if (offer.kind === 'subscription') {
        const trialEnd = new Date(now.getTime() + offer.trialDays * 86_400_000);
        const startsAt = offer.trialDays ? trialEnd : now;
        const endsAt = anchoredServiceEnd({ startsAt, interval: offer.interval!, timeZone: 'UTC', anchorDay: startsAt.getUTCDate(), anchorTime: `${String(startsAt.getUTCHours()).padStart(2, '0')}:${String(startsAt.getUTCMinutes()).padStart(2, '0')}` });
        await tx.insert(schema.billingMembership).values({ orderId, state: offer.trialDays ? 'trial' : offer.accessPolicy === 'invoice_first' ? 'active' : 'pending', serviceStartsAt: startsAt, serviceEndsAt: endsAt });
      }
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
  const atoms = positiveAtoms(field(form, 'amountAtoms')), operationKey = field(form, 'operationKey'), invoiceId = field(form, 'invoice'), evidence = field(form, 'evidence').trim();
  if (!atoms || !operationKey || evidence.length < 3 || evidence.length > 2000) return { error: 'Enter a positive amount and identifying payment evidence.' };
  try {
    await database().transaction(async (tx) => {
      const order = (await tx.select().from(schema.billingOrder).where(and(eq(schema.billingOrder.invoiceId, invoiceId), eq(schema.billingOrder.customerPersonId, me.personId))).limit(1))[0];
      if (!order) throw new Error('invoice');
      await tx.insert(schema.billingReceiptClaim).values({ operationKey, customerPersonId: me.personId, invoiceId, amountAtoms: atoms, evidence });
      await emit(tx, { orgId: 'isoastra', resourceKind: 'billing-receipt', resourceId: operationKey, kind: 'claimed' });
    });
    revalidatePath(userPath(customerId(me.personId), 'billing'));
    return { notice: 'Payment evidence submitted for confirmation.' };
  } catch { return { error: DECLINED }; }
}

export async function confirmReceipt(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const claimId = field(form, 'claim'), operationKey = field(form, 'operationKey');
  try {
    await database().transaction(async (tx) => {
      const row = (await tx.select({ claim: schema.billingReceiptClaim, order: schema.billingOrder, seller: schema.billingSeller, terms: schema.billingOfferVersion.terms })
        .from(schema.billingReceiptClaim).innerJoin(schema.billingOrder, eq(schema.billingReceiptClaim.invoiceId, schema.billingOrder.invoiceId))
        .innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id))
        .innerJoin(schema.billingOfferVersion, and(eq(schema.billingOrder.offerNamespace, schema.billingOfferVersion.namespace), eq(schema.billingOrder.offerId, schema.billingOfferVersion.id), eq(schema.billingOrder.offerVersion, schema.billingOfferVersion.version)))
        .where(and(eq(schema.billingReceiptClaim.id, claimId), eq(schema.billingReceiptClaim.state, 'pending'))).limit(1))[0];
      if (!row) throw new Error('claim');
      const now = new Date(), chart = row.seller.chart as AppChart;
      const receipt = await recordPayment(ledger(tx), { bookId: row.seller.bookId, operationKey, paymentId: `manual:${claimId}`, provider: 'manual', economicKey: claimId, providerEventId: operationKey, customerId: customerId(row.order.customerPersonId), invoiceId: row.order.invoiceId, unit: 'USD', atoms: row.claim.amountAtoms, occurredAt: now, effectiveOn: dateOnly(now), chart });
      const paid = row.order.paidAtoms + receipt.appliedAtoms;
      await tx.update(schema.billingReceiptClaim).set({ state: 'confirmed', confirmedAt: now }).where(and(eq(schema.billingReceiptClaim.id, claimId), eq(schema.billingReceiptClaim.state, 'pending')));
      await tx.update(schema.billingOrder).set({ paidAtoms: paid, state: paid >= row.order.totalAtoms ? 'paid' : 'partially_paid', version: sql`${schema.billingOrder.version}+1` }).where(eq(schema.billingOrder.id, row.order.id));
      if (paid >= row.order.totalAtoms && row.order.kind === 'subscription') {
        const offer = materializeOffer(row.order.sellerId, row.order.offerVersion, catalogSchema.shape.offers.element.parse(row.terms));
        const membership = (await tx.select().from(schema.billingMembership).where(eq(schema.billingMembership.orderId, row.order.id)).limit(1))[0];
        if (membership && offer.accessPolicy === 'payment_first' && offer.trialDays === 0 && row.order.renewsOrderId === null) {
          const end = anchoredServiceEnd({ startsAt: now, interval: offer.interval!, timeZone: 'UTC', anchorDay: membership.serviceStartsAt.getUTCDate(), anchorTime: `${String(membership.serviceStartsAt.getUTCHours()).padStart(2, '0')}:${String(membership.serviceStartsAt.getUTCMinutes()).padStart(2, '0')}` });
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
      const row = (await tx.select({ order: schema.billingOrder, seller: schema.billingSeller }).from(schema.billingOrder).innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id)).where(and(eq(schema.billingOrder.id, orderId), eq(schema.billingOrder.version, version))).limit(1))[0];
      if (!row || row.order.kind !== 'one_time' || row.order.fulfilledAt) throw new Error('order');
      const now = new Date();
      await recognize(tx, row.seller.bookId, row.seller.chart as AppChart, row.order.totalAtoms, operationKey, `order:${orderId}`, now);
      await tx.update(schema.billingOrder).set({ fulfilledAt: now, state: 'fulfilled', version: version + 1 }).where(and(eq(schema.billingOrder.id, orderId), eq(schema.billingOrder.version, version)));
      await recordAudit(tx, { actorPersonId: principal.personId, command: 'fulfill-order', targetKind: 'person', targetId: row.order.customerPersonId, payload: { orderId } });
    });
    revalidatePath(operatorPath('billing'));
    return { notice: 'Fulfillment recorded.' };
  } catch { return { error: DECLINED }; }
}

export async function changeMembership(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await member();
  const membershipId = field(form, 'membership'), offerId = field(form, 'offer'), version = expected(form);
  try {
    const changed = await database().transaction(async (tx) => {
      const row = (await tx.select({ membership: schema.billingMembership, order: schema.billingOrder }).from(schema.billingMembership).innerJoin(schema.billingOrder, eq(schema.billingMembership.orderId, schema.billingOrder.id)).where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version), eq(schema.billingOrder.customerPersonId, me.personId))).limit(1))[0];
      if (!row) return false;
      const target = await sellerAndOffer(tx, row.order.sellerId, offerId);
      if (!target || target.offer.kind !== 'subscription') return false;
      await tx.update(schema.billingMembership).set({ pendingOfferId: target.offer.id, pendingOfferVersion: target.offer.version, version: version + 1 }).where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version)));
      return true;
    });
    if (!changed) return { error: DECLINED };
    revalidatePath(userPath(customerId(me.personId), 'billing'));
    return { notice: 'Tier change scheduled for the next cycle.' };
  } catch { return { error: DECLINED }; }
}

export async function cancelRenewal(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await member(), membershipId = field(form, 'membership'), version = expected(form);
  const rows = await database().update(schema.billingMembership).set({ cancelAt: sql`${schema.billingMembership.serviceEndsAt}`, version: version + 1 }).from(schema.billingOrder)
    .where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version), eq(schema.billingMembership.orderId, schema.billingOrder.id), eq(schema.billingOrder.customerPersonId, me.personId))).returning({ id: schema.billingMembership.id });
  if (!rows.length) return { error: DECLINED };
  revalidatePath(userPath(customerId(me.personId), 'billing'));
  return { notice: 'Renewal cancelled. Existing balances remain due.' };
}

export async function setMembershipSuspended(_prev: FormState, form: FormData): Promise<FormState> {
  await requireOperator();
  const membershipId = field(form, 'membership'), version = expected(form), suspend = field(form, 'suspend') === 'true';
  const rows = await database().update(schema.billingMembership).set({ suspendedAt: suspend ? new Date() : null, state: suspend ? 'suspended' : 'active', version: version + 1 })
    .where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version))).returning({ id: schema.billingMembership.id });
  if (!rows.length) return { error: DECLINED };
  revalidatePath(operatorPath('billing'));
  return { notice: suspend ? 'Membership suspended.' : 'Membership resumed.' };
}

export async function renewMembership(_prev: FormState, form: FormData): Promise<FormState> {
  await requireOperator();
  const membershipId = field(form, 'membership'), version = expected(form), operationKey = field(form, 'operationKey');
  try {
    await database().transaction(async (tx) => {
      const row = (await tx.select({ membership: schema.billingMembership, order: schema.billingOrder, seller: schema.billingSeller })
        .from(schema.billingMembership).innerJoin(schema.billingOrder, eq(schema.billingMembership.orderId, schema.billingOrder.id)).innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id))
        .where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version))).limit(1))[0];
      const now = new Date();
      if (!row || now < row.membership.serviceEndsAt || row.membership.suspendedAt) throw new Error('membership');
      if (row.membership.cancelAt) {
        await tx.update(schema.billingMembership).set({ state: 'cancelled', version: version + 1 }).where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version)));
        return;
      }
      const nextOfferId = row.membership.pendingOfferId ?? row.order.offerId;
      const found = await sellerAndOffer(tx, row.order.sellerId, nextOfferId);
      if (!found || found.offer.kind !== 'subscription') throw new Error('offer');
      const { offer, seller } = found, startsAt = row.membership.serviceEndsAt;
      const endsAt = anchoredServiceEnd({ startsAt, interval: offer.interval!, timeZone: 'UTC', anchorDay: row.membership.serviceStartsAt.getUTCDate(), anchorTime: `${String(row.membership.serviceStartsAt.getUTCHours()).padStart(2, '0')}:${String(row.membership.serviceStartsAt.getUTCMinutes()).padStart(2, '0')}` });
      const orderId = randomUUID(), invoiceId = `rn:${orderId}`, chart = seller.chart as AppChart;
      await issueInvoice(ledger(tx), { bookId: seller.bookId, operationKey: `invoice:${operationKey}`, invoiceId, customerId: customerId(row.order.customerPersonId), unit: 'USD', atoms: offer.amountAtoms, occurredAt: now, effectiveOn: dateOnly(now), chart: { ...chart, revenue: chart.deferredRevenue }, lines: [{ key: `${offer.namespace}:${offer.id}:${offer.version}`, description: `${offer.title} renewal`, atoms: offer.amountAtoms }] });
      await recognize(tx, seller.bookId, chart, row.order.totalAtoms, `recognize:${operationKey}`, `order:${row.order.id}`, now);
      await tx.insert(schema.billingOrder).values({ id: orderId, operationKey, customerPersonId: row.order.customerPersonId, sellerId: seller.id, offerNamespace: offer.namespace, offerId: offer.id, offerVersion: offer.version, invoiceId, kind: 'subscription', state: offer.accessPolicy === 'invoice_first' ? 'invoice_open' : 'invoice_open', totalAtoms: offer.amountAtoms, paidAtoms: 0n, renewsOrderId: row.order.id });
      await tx.update(schema.billingMembership).set({ orderId, state: offer.accessPolicy === 'invoice_first' ? 'active' : 'pending', serviceStartsAt: startsAt, serviceEndsAt: endsAt, pendingOfferId: null, pendingOfferVersion: null, version: version + 1 }).where(and(eq(schema.billingMembership.id, membershipId), eq(schema.billingMembership.version, version)));
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
  const orderId = field(form, 'order'), operationKey = field(form, 'operationKey'), atoms = positiveAtoms(field(form, 'amountAtoms'));
  if (!atoms) return { error: DECLINED };
  try {
    await database().transaction(async (tx) => {
      const row = (await tx.select({ order: schema.billingOrder, seller: schema.billingSeller }).from(schema.billingOrder).innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id)).where(eq(schema.billingOrder.id, orderId)).limit(1))[0];
      if (!row || atoms > row.order.totalAtoms) throw new Error('order');
      const now = new Date();
      await issueCreditNote(ledger(tx), { creditNoteId: `credit:${operationKey}`, invoiceId: row.order.invoiceId, bookId: row.seller.bookId, operationKey, unit: 'USD', atoms, occurredAt: now, effectiveOn: dateOnly(now), chart: row.seller.chart as AppChart });
      await tx.insert(schema.billingAdjustment).values({ operationKey, orderId, kind: 'credit', amountAtoms: atoms, evidence: field(form, 'evidence') });
      await recordAudit(tx, { actorPersonId: principal.personId, command: 'credit-order', targetKind: 'person', targetId: row.order.customerPersonId, payload: { orderId, atoms: atoms.toString() } });
    });
    revalidatePath(operatorPath('billing'));
    return { notice: 'Credit note recorded.' };
  } catch { return { error: DECLINED }; }
}

export async function recordExternalRefund(_prev: FormState, form: FormData): Promise<FormState> {
  const { principal } = await requireOperator();
  const orderId = field(form, 'order'), operationKey = field(form, 'operationKey'), atoms = positiveAtoms(field(form, 'amountAtoms')), evidence = field(form, 'evidence').trim();
  if (!atoms || evidence.length < 3) return { error: DECLINED };
  try {
    await database().transaction(async (tx) => {
      const row = (await tx.select({ order: schema.billingOrder, seller: schema.billingSeller }).from(schema.billingOrder).innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id)).where(eq(schema.billingOrder.id, orderId)).limit(1))[0];
      if (!row || atoms > row.order.paidAtoms) throw new Error('refund');
      const chart = row.seller.chart as AppChart, now = new Date();
      await postJournal(ledger(tx), { bookId: row.seller.bookId, operationKey, kind: 'billing.external_refund', occurredAt: now, effectiveOn: dateOnly(now), evidence: [{ kind: 'external-refund', id: evidence }], policy: { key: 'billing.external-refund', version: 1 }, postings: [{ accountId: chart.contraRevenue, unit: 'USD', atoms }, { accountId: chart.processorClearing, unit: 'USD', atoms: -atoms }] });
      await tx.insert(schema.billingAdjustment).values({ operationKey, orderId, kind: 'refund', amountAtoms: atoms, evidence });
      await recordAudit(tx, { actorPersonId: principal.personId, command: 'record-external-refund', targetKind: 'person', targetId: row.order.customerPersonId, payload: { orderId, atoms: atoms.toString(), evidence } });
    });
    revalidatePath(operatorPath('billing'));
    return { notice: 'Externally completed refund recorded.' };
  } catch { return { error: DECLINED }; }
}
