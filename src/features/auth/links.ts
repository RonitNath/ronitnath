/* Single-use URLs. One table for all of them (docs/plan.md §Model): a token
 * hash, a kind, what it points at, when it dies and when it was spent.
 *
 * Spending a link is an UPDATE guarded on `used_at IS NULL`, so two clicks
 * race in the database and exactly one of them wins. Nothing reads the link
 * first and writes it after. */

import { and, eq, gt, isNull, sql } from 'drizzle-orm';

import { schema } from '@/db/client';
import { publicOrigin } from '@/lib/env';
import type { Transaction } from './db';
import { hashToken, mintToken, tokenLooksWellFormed } from './secrets';

export const VERIFY_TTL_HOURS = 24;
export const RESET_TTL_HOURS = 1;

export type IssuedKind = 'verify_email' | 'reset_password';

export async function issueLink(
  tx: Transaction,
  input: { kind: IssuedKind; targetKind: string; targetId: number; ttlHours: number; createdBy?: number | null },
): Promise<string> {
  const token = mintToken();
  await tx.insert(schema.link).values({
    tokenHash: hashToken(token),
    kind: input.kind,
    targetKind: input.targetKind,
    targetId: input.targetId,
    createdBy: input.createdBy ?? null,
    expiresAt: new Date(Date.now() + input.ttlHours * 3_600_000),
  });
  return token;
}

export function linkUrl(kind: IssuedKind, token: string): string {
  const path = kind === 'verify_email' ? `/auth/verify/${token}` : `/auth/reset/${token}`;
  return `${publicOrigin()}${path}`;
}

export interface SpentLink {
  targetKind: string;
  targetId: number;
}

/** Mark a live link used and say what it pointed at, or null. The guard is
 *  the whole rule: a second click updates nothing and gets null. */
export async function spendLink(
  tx: Transaction,
  kind: IssuedKind,
  token: string,
): Promise<SpentLink | null> {
  if (!tokenLooksWellFormed(token)) return null;
  const rows = await tx
    .update(schema.link)
    .set({ usedAt: sql`now()` })
    .where(
      and(
        eq(schema.link.tokenHash, hashToken(token)),
        eq(schema.link.kind, kind),
        isNull(schema.link.usedAt),
        gt(schema.link.expiresAt, sql`now()`),
      ),
    )
    .returning({ targetKind: schema.link.targetKind, targetId: schema.link.targetId });
  return rows[0] ?? null;
}
