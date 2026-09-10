/* Reads. Separate from actions.ts on purpose: everything exported from a
 * `'use server'` module is a callable endpoint, and a query does not need to
 * be one. */

import { eq } from 'drizzle-orm';
import { headers } from 'next/headers';

import { database, schema } from '@/db/client';
import { auth } from './auth';
import { hashToken } from './secrets';

export interface SessionRow {
  id: string;
  /* What the revoke button sends back: the SHA-256 of the session's token.
   * The token itself is the key to that session, so a page that printed one
   * would be handing it out; a digest names the row and opens nothing. */
  handle: string;
  userAgent: string | null;
  ip: string | null;
  createdAt: Date;
  lastSeenAt: Date;
  expiresAt: Date;
}

/** Live sessions the reader holds, most recently touched first. Better-auth
 *  owns the rows and the expiry, so this asks it rather than the table:
 *  `listSessions` already filters the expired ones out. */
export async function listSessions(): Promise<SessionRow[]> {
  const rows = await auth().api.listSessions({ headers: await headers() });
  return rows
    .map((row) => ({
      id: row.id,
      handle: hashToken(row.token),
      userAgent: row.userAgent ?? null,
      ip: row.ipAddress ?? null,
      createdAt: new Date(row.createdAt),
      /* Better-auth touches `updated_at` when it slides a session, which is
       * the same fact the old `last_seen_at` column carried. */
      lastSeenAt: new Date(row.updatedAt),
      expiresAt: new Date(row.expiresAt),
    }))
    .sort((a, b) => b.lastSeenAt.getTime() - a.lastSeenAt.getTime());
}

/** The identities on file for a person: the address their account is reached
 *  at, and every handle somebody wrote down for them. Neither is a door any
 *  more — signing in is `auth.user` and `auth.account` — but the match queue
 *  is written in terms of these rows (src/features/auth/mirror.ts). */
export function listIdentities(personId: number) {
  return database()
    .select({
      id: schema.identity.id,
      source: schema.identity.source,
      subject: schema.identity.subject,
      verifiedAt: schema.identity.verifiedAt,
      createdAt: schema.identity.createdAt,
    })
    .from(schema.identity)
    .where(eq(schema.identity.personId, personId))
    .orderBy(schema.identity.id);
}
