import { and, asc, desc, eq, inArray } from 'drizzle-orm';
import { database, schema } from '@/db/client';
import { catalogSchema, materializeOffer } from './model';

export async function sellers() {
  return database().select().from(schema.billingSeller).orderBy(asc(schema.billingSeller.name));
}

export async function publishedOffers() {
  const rows = await database().select().from(schema.billingOffer)
    .innerJoin(schema.billingOfferVersion, and(
      eq(schema.billingOffer.namespace, schema.billingOfferVersion.namespace),
      eq(schema.billingOffer.id, schema.billingOfferVersion.id),
      eq(schema.billingOffer.publishedVersion, schema.billingOfferVersion.version),
    )).innerJoin(schema.billingSeller, eq(schema.billingOfferVersion.sellerId, schema.billingSeller.id))
    .where(and(eq(schema.billingOffer.available, true), eq(schema.billingSeller.enabled, true)))
    .orderBy(asc(schema.billingSeller.name), asc(schema.billingOfferVersion.id));
  return rows.map(({ billing_offer_version: row, billing_seller: seller }) => ({ seller, offer: materializeOffer(row.sellerId, row.version, catalogSchema.shape.offers.element.parse(row.terms)) }));
}

export async function billingForCustomer(personId: number) {
  const orders = await database().select().from(schema.billingOrder).where(eq(schema.billingOrder.customerPersonId, personId)).orderBy(desc(schema.billingOrder.createdAt));
  const orderIds = orders.map((row) => row.id);
  const [memberships, claims, adjustments] = await Promise.all([
    orderIds.length ? database().select().from(schema.billingMembership).where(inArray(schema.billingMembership.orderId, orderIds)) : [],
    database().select().from(schema.billingReceiptClaim).where(eq(schema.billingReceiptClaim.customerPersonId, personId)).orderBy(desc(schema.billingReceiptClaim.createdAt)),
    orderIds.length ? database().select().from(schema.billingAdjustment).where(inArray(schema.billingAdjustment.orderId, orderIds)) : [],
  ]);
  return { orders, memberships, claims, adjustments };
}

export async function reconciliationQueue() {
  return database().select({ claim: schema.billingReceiptClaim, order: schema.billingOrder, person: schema.person })
    .from(schema.billingReceiptClaim)
    .innerJoin(schema.billingOrder, eq(schema.billingReceiptClaim.invoiceId, schema.billingOrder.invoiceId))
    .innerJoin(schema.person, eq(schema.billingReceiptClaim.customerPersonId, schema.person.id))
    .where(eq(schema.billingReceiptClaim.state, 'pending')).orderBy(asc(schema.billingReceiptClaim.createdAt));
}

export async function operatorOrders() {
  return database().select({ order: schema.billingOrder, person: schema.person, seller: schema.billingSeller })
    .from(schema.billingOrder).innerJoin(schema.person, eq(schema.billingOrder.customerPersonId, schema.person.id))
    .innerJoin(schema.billingSeller, eq(schema.billingOrder.sellerId, schema.billingSeller.id))
    .orderBy(desc(schema.billingOrder.createdAt)).limit(100);
}

export async function operatorMemberships() {
  return database().select({ membership: schema.billingMembership, order: schema.billingOrder, person: schema.person })
    .from(schema.billingMembership).innerJoin(schema.billingOrder, eq(schema.billingMembership.orderId, schema.billingOrder.id))
    .innerJoin(schema.person, eq(schema.billingOrder.customerPersonId, schema.person.id)).orderBy(asc(schema.billingMembership.serviceEndsAt));
}
