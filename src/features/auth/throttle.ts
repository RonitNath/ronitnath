/* Attempt counting, in Postgres because Postgres is the only store this
 * deployment has. A fixed window per (scope, key): the row carries when the
 * window opened and how many attempts landed in it, and one UPSERT both opens
 * a fresh window and counts into the current one.
 *
 * Keys are per email *and* per IP, counted separately, so one address under
 * attack cannot lock out a shared address and one office NAT cannot lock out
 * a person who is typing their own password correctly. */

import { and, eq, sql } from 'drizzle-orm';
import { schema } from '@/db/client';
import { hashToken } from './secrets';
import type { Transaction } from './db';

export interface Limit {
  scope: string;
  /* Attempts allowed against one address in a window. */
  max: number;
  /* Attempts allowed from one address of origin in the same window. */
  ipMax: number;
  windowSeconds: number;
}

/* Two counters per command, and the address is the tight one. An IP is a
 * building — an office NAT, a campus, a phone carrier — so a limit low enough
 * to stop guessing one password is low enough to lock out everybody behind it;
 * the per-address counter is what actually protects an account. */
export const SIGN_IN_LIMIT: Limit = { scope: 'sign_in', max: 10, ipMax: 120, windowSeconds: 900 };
export const RESET_LIMIT: Limit = { scope: 'reset', max: 5, ipMax: 60, windowSeconds: 3600 };

/* Addresses are not stored in the clear here: the counter needs to tell two
 * keys apart, not to say what they were. */
function keyOf(parts: string[]): string {
  return hashToken(parts.join(' '));
}

/** Count one attempt. `true` means the caller is over the limit and the
 *  command must decline without doing its work. */
export async function overLimit(tx: Transaction, limit: Limit, parts: string[]): Promise<boolean> {
  const key = keyOf(parts);
  const fresh = sql`${schema.authThrottle.windowStartedAt} > now() - (${limit.windowSeconds} * interval '1 second')`;
  const rows = await tx
    .insert(schema.authThrottle)
    .values({ scope: limit.scope, key, count: 1 })
    .onConflictDoUpdate({
      target: [schema.authThrottle.scope, schema.authThrottle.key],
      set: {
        count: sql`case when ${fresh} then ${schema.authThrottle.count} + 1 else 1 end`,
        windowStartedAt: sql`case when ${fresh} then ${schema.authThrottle.windowStartedAt} else now() end`,
      },
    })
    .returning({ count: schema.authThrottle.count });

  const ceiling = parts[0] === 'ip' ? limit.ipMax : limit.max;
  return (rows[0]?.count ?? 0) > ceiling;
}

/** A successful sign-in clears its own counters. */
export async function clearLimit(tx: Transaction, limit: Limit, parts: string[]): Promise<void> {
  await tx
    .delete(schema.authThrottle)
    .where(
      and(eq(schema.authThrottle.scope, limit.scope), eq(schema.authThrottle.key, keyOf(parts))),
    );
}
