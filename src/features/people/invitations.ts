/* Invitations. A claim link is the one URL this site hands out that is not a
 * one-shot errand: it is how a member reaches somebody who has no account, so
 * it lives for a month, it is watched (opened, claimed) and it can be taken
 * back.
 *
 * It is minted, shown once, and never stored — the row keeps the SHA-256, and
 * a member who loses the URL mints another one. The page says so, because a
 * "copy link" button that quietly cannot show you the link again is a trap. */

import { and, eq, gt, isNull, sql } from 'drizzle-orm';

import { schema } from '@/db/client';
import { hashToken, mintToken, tokenLooksWellFormed } from '@/features/auth/secrets';
import type { Transaction } from '@/features/auth/db';
import { publicOrigin } from '@/lib/env';

export const CLAIM_TTL_DAYS = 30;

export function claimUrl(token: string): string {
  return `${publicOrigin()}/links/${token}`;
}

export async function mintClaimLink(
  tx: Transaction,
  input: { personId: number; createdBy: number; ttlDays?: number },
): Promise<{ token: string; linkId: number }> {
  const token = mintToken();
  const rows = await tx
    .insert(schema.link)
    .values({
      tokenHash: hashToken(token),
      kind: 'claim',
      targetKind: 'person',
      targetId: input.personId,
      createdBy: input.createdBy,
      expiresAt: new Date(Date.now() + (input.ttlDays ?? CLAIM_TTL_DAYS) * 86_400_000),
    })
    .returning({ id: schema.link.id });
  return { token, linkId: rows[0]!.id };
}

/* Every read and every write of a claim link carries the same guard, so that
 * expired, revoked, claimed and never-existed are one answer with one shape:
 * the visitor is told the link does not work, and nothing else. */
function live(token: string) {
  return and(
    eq(schema.link.tokenHash, hashToken(token)),
    eq(schema.link.kind, 'claim'),
    isNull(schema.link.claimedAt),
    isNull(schema.link.revokedAt),
    gt(schema.link.expiresAt, sql`now()`),
  );
}

export interface OpenLink {
  linkId: number;
  personId: number;
  createdBy: number | null;
}

/** Stamp the first sight of a link and say who it points at. A mail scanner
 *  that follows the URL stamps it too, which is the price of knowing the
 *  difference between a link nobody has looked at and one that went nowhere. */
export async function openClaimLink(tx: Transaction, token: string): Promise<OpenLink | null> {
  if (!tokenLooksWellFormed(token)) return null;
  const rows = await tx
    .update(schema.link)
    .set({ openedAt: sql`coalesce(${schema.link.openedAt}, now())` })
    .where(live(token))
    .returning({
      linkId: schema.link.id,
      personId: schema.link.targetId,
      createdBy: schema.link.createdBy,
    });
  return rows[0] ?? null;
}

/** Spend a claim link. The guard is the whole rule: two clicks race in the
 *  database and exactly one of them wins. */
export async function spendClaimLink(
  tx: Transaction,
  token: string,
  /* Null when the claimant does not exist yet — the register-and-claim path
   * stamps it as soon as the person row does. */
  claimedBy: number | null,
): Promise<OpenLink | null> {
  if (!tokenLooksWellFormed(token)) return null;
  const rows = await tx
    .update(schema.link)
    .set({ claimedAt: sql`now()`, claimedBy, openedAt: sql`coalesce(${schema.link.openedAt}, now())` })
    .where(live(token))
    .returning({
      linkId: schema.link.id,
      personId: schema.link.targetId,
      createdBy: schema.link.createdBy,
    });
  return rows[0] ?? null;
}

/** What `/app/people` says about one invitation. A state is a word. */
export type LinkState = 'live' | 'opened' | 'claimed' | 'revoked' | 'expired';

export function linkState(row: {
  openedAt: Date | null;
  claimedAt: Date | null;
  revokedAt: Date | null;
  expiresAt: Date | null;
}): LinkState {
  if (row.claimedAt) return 'claimed';
  if (row.revokedAt) return 'revoked';
  if (row.expiresAt && row.expiresAt.getTime() <= Date.now()) return 'expired';
  return row.openedAt ? 'opened' : 'live';
}
