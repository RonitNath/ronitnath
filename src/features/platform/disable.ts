/* Disable and Enable, as the two halves of one fact.
 *
 * A disabled party keeps every row it has and loses every door. What that
 * costs is not a guess: the sessions it ends and the links it stops are
 * counted as they are written, and their ids go into the audit row — which is
 * how Enable can put back exactly what this took away rather than everything
 * that happens to be revoked.
 *
 * An organization's members are untouched. What goes is the organization's
 * surface, which `organizationByHandle` answers for. */

import { and, eq, isNull, or, sql } from 'drizzle-orm';

import { schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';

export interface Cascade {
  kind: 'person' | 'organization' | 'service';
  sessions: number;
  links: number[];
}

/** Stamp the party and take its doors. Null when it was already disabled or
 *  is not there — one answer for both, as everywhere else. */
export async function disableCascade(
  tx: Transaction,
  partyId: number,
): Promise<Cascade | null> {
  const rows = await tx
    .update(schema.party)
    .set({ disabledAt: sql`now()` })
    .where(and(eq(schema.party.id, partyId), isNull(schema.party.disabledAt)))
    .returning({ kind: schema.party.kind });
  if (rows.length === 0) return null;

  const sessions = await tx
    .update(schema.session)
    .set({ revokedAt: sql`now()` })
    .where(and(eq(schema.session.personId, partyId), isNull(schema.session.revokedAt)))
    .returning({ id: schema.session.id });

  /* Every URL this party is either the author or the subject of. A bearer
   * link outlives the person who minted it unless something says otherwise,
   * and a claimed one is history rather than a door. */
  const links = await tx
    .update(schema.link)
    .set({ revokedAt: sql`now()` })
    .where(
      and(
        isNull(schema.link.revokedAt),
        isNull(schema.link.claimedAt),
        or(
          eq(schema.link.createdBy, partyId),
          and(eq(schema.link.targetKind, 'person'), eq(schema.link.targetId, partyId)),
        ),
      ),
    )
    .returning({ id: schema.link.id });

  return { kind: rows[0]!.kind, sessions: sessions.length, links: links.map((row) => row.id) };
}

/** Give the doors back. Sessions are not restored — they are re-mintable by
 *  signing in, and a session nobody was holding when it ended is not one
 *  anybody is waiting for. Links are, because a link is somebody else's only
 *  copy. */
export async function enableRestore(tx: Transaction, partyId: number): Promise<number | null> {
  const rows = await tx
    .update(schema.party)
    .set({ disabledAt: null })
    .where(eq(schema.party.id, partyId))
    .returning({ id: schema.party.id });
  if (rows.length === 0) return null;

  const last = await tx
    .select({ payload: schema.audit.payload })
    .from(schema.audit)
    .where(and(eq(schema.audit.command, 'disable-party'), eq(schema.audit.targetId, partyId)))
    .orderBy(sql`${schema.audit.id} desc`)
    .limit(1);
  const ids = (last[0]?.payload as { links_revoked?: number[] } | null)?.links_revoked ?? [];

  let restored = 0;
  for (const id of ids) {
    const back = await tx
      .update(schema.link)
      .set({ revokedAt: null })
      .where(and(eq(schema.link.id, id), isNull(schema.link.claimedAt)))
      .returning({ id: schema.link.id });
    restored += back.length;
  }
  return restored;
}
