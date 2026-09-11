import { z } from 'zod';
import type { OfferVersion } from '@isoastra/fleet-billing/commerce';

const benefit = z.union([z.boolean(), z.string().regex(/^\d+$/)]);
export const offerInputSchema = z.object({
  id: z.string().regex(/^[a-z][a-z0-9-]{0,62}$/),
  title: z.string().trim().min(1).max(120),
  description: z.string().trim().max(2000).default(''),
  kind: z.enum(['one_time', 'subscription']),
  amountAtoms: z.string().regex(/^\d+$/),
  interval: z.enum(['month', 'year']).nullable().default(null),
  accessPolicy: z.enum(['payment_first', 'invoice_first']).default('payment_first'),
  trialDays: z.number().int().min(0).max(3660).default(0),
  paymentDueDays: z.number().int().min(0).max(3660).default(7),
  graceDays: z.number().int().min(0).max(3660).default(0),
  fulfillment: z.enum(['immediate', 'operator_confirmed']).default('operator_confirmed'),
  benefits: z.record(z.string().regex(/^[a-z][a-z0-9_.-]{0,63}$/), benefit).default({}),
  available: z.boolean().default(true),
});

export const catalogSchema = z.object({ offers: z.array(offerInputSchema).max(100) }).superRefine((body, ctx) => {
  const ids = new Set<string>();
  for (const [index, offer] of body.offers.entries()) {
    if (ids.has(offer.id)) ctx.addIssue({ code: 'custom', path: ['offers', index, 'id'], message: 'duplicate offer id' });
    ids.add(offer.id);
    if ((offer.kind === 'subscription') !== (offer.interval !== null)) {
      ctx.addIssue({ code: 'custom', path: ['offers', index, 'interval'], message: 'interval must be set only for subscriptions' });
    }
  }
});

export type Catalog = z.infer<typeof catalogSchema>;
export type OfferInput = z.infer<typeof offerInputSchema>;

export function materializeOffer(sellerId: string, version: number, offer: OfferInput): OfferVersion {
  return {
    ...offer,
    namespace: sellerId,
    sellerId,
    version,
    unit: 'USD',
    amountAtoms: BigInt(offer.amountAtoms),
    benefits: Object.fromEntries(Object.entries(offer.benefits).map(([key, value]) => [key, typeof value === 'boolean' ? value : BigInt(value)])),
  };
}

export function formatUsd(atoms: bigint): string {
  const absolute = atoms < 0n ? -atoms : atoms;
  return `${atoms < 0n ? '-' : ''}$${absolute / 100n}.${(absolute % 100n).toString().padStart(2, '0')}`;
}

export function dateOnly(value: Date): string {
  return value.toISOString().slice(0, 10);
}
