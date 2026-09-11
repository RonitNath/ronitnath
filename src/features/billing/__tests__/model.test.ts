import { describe, expect, it } from 'vitest';
import { anchoredServiceEnd, membershipAccess } from '@isoastra/fleet-billing/commerce';
import { catalogSchema, materializeOffer } from '../model';

const base = { id: 'member', title: 'Member', description: '', kind: 'subscription' as const, amountAtoms: '1000', interval: 'month' as const, accessPolicy: 'payment_first' as const, trialDays: 0, paymentDueDays: 7, graceDays: 0, fulfillment: 'operator_confirmed' as const, benefits: { 'social.private': true, invites: '4' }, available: true };

describe('billing configuration contract', () => {
  it('rejects duplicate ids and arbitrary benefit shapes', () => {
    expect(catalogSchema.safeParse({ offers: [base, base] }).success).toBe(false);
    expect(catalogSchema.safeParse({ offers: [{ ...base, benefits: { operator: -1 } }] }).success).toBe(false);
  });

  it('keeps month-end anchors and denies partial payment-first access', () => {
    const offer = materializeOffer('ronit', 3, catalogSchema.shape.offers.element.parse(base));
    const start = new Date('2028-01-31T10:15:00Z');
    const end = anchoredServiceEnd({ startsAt: start, interval: 'month', timeZone: 'UTC', anchorDay: 31, anchorTime: '10:15' });
    expect(end.toISOString()).toBe('2028-02-29T10:15:00.000Z');
    expect(membershipAccess({ offer, acceptedAt: start, serviceStartsAt: start, serviceEndsAt: end, paidAtoms: 999n, suspended: false, cancelled: false, now: new Date('2028-02-01T00:00:00Z') }).allowed).toBe(false);
  });

  it('ends an invoiced interval rather than shifting it after late payment', () => {
    const offer = materializeOffer('isoastra', 8, catalogSchema.shape.offers.element.parse(base));
    const result = membershipAccess({ offer, acceptedAt: new Date('2028-01-01T00:00:00Z'), serviceStartsAt: new Date('2028-02-01T00:00:00Z'), serviceEndsAt: new Date('2028-03-01T00:00:00Z'), paidAtoms: 1000n, suspended: false, cancelled: false, now: new Date('2028-02-20T00:00:00Z') });
    expect(result).toMatchObject({ allowed: true, until: new Date('2028-03-01T00:00:00Z') });
  });
});
