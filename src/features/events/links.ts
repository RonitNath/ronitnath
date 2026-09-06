/* The two URLs an event hands out.
 *
 * A **personal link** is the same shape as an invitation (`src/features/people/
 * invitations.ts`): it identifies one person without authenticating them, it
 * is watched, it can be taken back — but it is not spent, because a guest is
 * meant to come back to it and change their mind. So it is read with the same
 * live-guard and stamped `opened_at`, and `claimed_at` stays free for the day
 * that guest turns their entry into an account through /links.
 *
 * An **open link** points at the event rather than at a person: whoever holds
 * it types a name, and that name becomes a held person with a personal link
 * of their own. One per event, minted at publication.
 *
 * Both are shown once, at mint time. The row keeps the SHA-256 and nothing on
 * this side can show the URL again. */

import { and, eq, gt, isNull, or, sql } from 'drizzle-orm';

import { schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';
import { hashToken, mintToken, tokenLooksWellFormed } from '@/features/auth/secrets';
import { publicOrigin } from '@/lib/env';

/* Long enough that a link pasted in a chat in March still works for a party
 * in June, and finite because a bearer URL that never dies is a key. */
export const EVENT_LINK_TTL_DAYS = 180;

export function guestUrl(slug: string, token: string): string {
  return `${publicOrigin()}/e/${slug}?l=${token}`;
}

export function openUrl(slug: string, token: string): string {
  return guestUrl(slug, token);
}

async function mint(
  tx: Transaction,
  input: {
    kind: 'event_personal' | 'event_open';
    targetKind: string;
    targetId: number;
    createdBy: number;
  },
): Promise<{ token: string; linkId: number }> {
  const token = mintToken();
  const rows = await tx
    .insert(schema.link)
    .values({
      tokenHash: hashToken(token),
      kind: input.kind,
      targetKind: input.targetKind,
      targetId: input.targetId,
      createdBy: input.createdBy,
      expiresAt: new Date(Date.now() + EVENT_LINK_TTL_DAYS * 86_400_000),
    })
    .returning({ id: schema.link.id });
  return { token, linkId: rows[0]!.id };
}

export function mintPersonalLink(
  tx: Transaction,
  input: { personId: number; createdBy: number },
) {
  return mint(tx, {
    kind: 'event_personal',
    targetKind: 'person',
    targetId: input.personId,
    createdBy: input.createdBy,
  });
}

export function mintOpenLink(tx: Transaction, input: { eventId: number; createdBy: number }) {
  return mint(tx, {
    kind: 'event_open',
    targetKind: 'event',
    targetId: input.eventId,
    createdBy: input.createdBy,
  });
}

/* One guard for both kinds, so revoked, expired and never-existed are one
 * answer with one shape. */
function live(token: string) {
  return and(
    eq(schema.link.tokenHash, hashToken(token)),
    or(eq(schema.link.kind, 'event_personal'), eq(schema.link.kind, 'event_open')),
    isNull(schema.link.revokedAt),
    gt(schema.link.expiresAt, sql`now()`),
  );
}

export interface Bearer {
  linkId: number;
  kind: 'event_personal' | 'event_open';
  /* A person for a personal link, an event for an open one. */
  targetId: number;
  createdBy: number | null;
}

/** Who the URL says is knocking, stamping the first sight of it. A personal
 *  link is never spent: the guest comes back to change their answer. */
export async function readEventLink(tx: Transaction, token: string): Promise<Bearer | null> {
  if (!tokenLooksWellFormed(token)) return null;
  const rows = await tx
    .update(schema.link)
    .set({ openedAt: sql`coalesce(${schema.link.openedAt}, now())` })
    .where(live(token))
    .returning({
      linkId: schema.link.id,
      kind: schema.link.kind,
      targetId: schema.link.targetId,
      createdBy: schema.link.createdBy,
    });
  const row = rows[0];
  if (!row || (row.kind !== 'event_personal' && row.kind !== 'event_open')) return null;
  return { linkId: row.linkId, kind: row.kind, targetId: row.targetId, createdBy: row.createdBy };
}
