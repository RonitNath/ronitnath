/* Reads. Separate from actions.ts on purpose: everything exported from a
 * `'use server'` module is a callable endpoint, and a query does not need to
 * be one. */

import { and, desc, eq, isNull, sql } from 'drizzle-orm';

import { database, schema } from '@/db/client';

/** Live sessions this person holds, most recently seen first. */
export function listSessions(personId: number) {
  return database()
    .select({
      id: schema.session.id,
      userAgent: schema.session.userAgent,
      ip: schema.session.ip,
      source: schema.session.source,
      lastSeenAt: schema.session.lastSeenAt,
      createdAt: schema.session.createdAt,
      expiresAt: schema.session.expiresAt,
    })
    .from(schema.session)
    .where(
      and(
        eq(schema.session.personId, personId),
        isNull(schema.session.revokedAt),
        sql`${schema.session.expiresAt} > now()`,
      ),
    )
    .orderBy(desc(schema.session.lastSeenAt));
}

/** The identities behind a person: which door each one is. */
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
