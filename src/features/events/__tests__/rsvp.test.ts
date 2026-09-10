/* The upsert, against a real database, because "one row per person per event"
 * is a property of the unique index and of `onConflictDoUpdate` — a fake would
 * be testing the fake. The suite skips itself when no database is listening,
 * so `pnpm gate` is green on a machine with nothing running; the Playwright
 * run always has one. */

import { sql } from 'drizzle-orm';
import { and, eq } from 'drizzle-orm';
import { afterAll, describe, expect, it } from 'vitest';

import { database, pool, schema } from '@/db/client';
import { createPerson } from '@/features/people/provision';

const reachable = await (async () => {
  if (!process.env.DATABASE_URL) return false;
  try {
    await database().execute(sql`select 1`);
    return true;
  } catch {
    return false;
  }
})();

afterAll(async () => {
  if (reachable) await pool().end();
});

describe.skipIf(!reachable)('rsvp', () => {
  it('keeps one row per person per event, and the last answer wins', async () => {
    const db = database();
    const { eventId, personId } = await db.transaction(async (tx) => {
      const host = await createPerson(tx, { displayName: 'Upsert Host' });
      const guest = await createPerson(tx, { displayName: 'Upsert Guest', held: true });
      const rows = await tx
        .insert(schema.event)
        .values({
          hostPersonId: host,
          slug: `upsert-${Date.now()}-${Math.floor(Math.random() * 1e6)}`,
          title: 'Upsert',
          startsAt: new Date('2026-08-30T21:00:00Z'),
        })
        .returning({ id: schema.event.id });
      return { eventId: rows[0]!.id, personId: guest };
    });

    const answer = async (response: 'yes' | 'maybe' | 'no', plusOne: number, note: string) =>
      db
        .insert(schema.rsvp)
        .values({ eventId, personId, response, plusOne, note })
        .onConflictDoUpdate({
          target: [schema.rsvp.eventId, schema.rsvp.personId],
          set: { response, plusOne, note, answeredAt: sql`now()` },
        });

    await answer('maybe', 0, 'probably');
    await answer('yes', 2, 'bringing a cake');

    const rows = await db
      .select({
        response: schema.rsvp.response,
        plusOne: schema.rsvp.plusOne,
        note: schema.rsvp.note,
      })
      .from(schema.rsvp)
      .where(and(eq(schema.rsvp.eventId, eventId), eq(schema.rsvp.personId, personId)));

    expect(rows).toHaveLength(1);
    expect(rows[0]).toEqual({ response: 'yes', plusOne: 2, note: 'bringing a cake' });

    await db.delete(schema.event).where(eq(schema.event.id, eventId));
    await db.delete(schema.person).where(eq(schema.person.id, personId));
  });
});
