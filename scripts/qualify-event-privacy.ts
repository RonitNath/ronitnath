import { sql } from 'drizzle-orm';

import { database, pool, schema } from '../src/db/client';
import { hostedEvent } from '../src/features/events/authority';
import { createPerson } from '../src/features/people/provision';
import { writeEventContent } from '../src/features/privacy/event-content';

const CANARY = 'GRID-PRIVATE-EVENT-7c640459-8dc8-44ec-a87c-8ccbaad31baa';

class CanaryComplete extends Error {}

async function main(): Promise<void> {
  let passed = false;
  try {
    await database().transaction(async (tx) => {
      const personId = await createPerson(tx, { displayName: 'Privacy qualification host' });
      const rows = await tx
        .insert(schema.event)
        .values({
          hostPersonId: personId,
          slug: `privacy-canary-${Date.now()}`,
          privacyRevision: 1,
        })
        .returning({
          id: schema.event.id,
          hostPersonId: schema.event.hostPersonId,
          slug: schema.event.slug,
          sequence: schema.event.sequence,
          privacyRevision: schema.event.privacyRevision,
          publishedAt: schema.event.publishedAt,
          updatedAt: schema.event.updatedAt,
          createdAt: schema.event.createdAt,
        });
      const row = rows[0]!;
      await writeEventContent(
        tx,
        row,
        {
          title: CANARY,
          summary: `${CANARY}-summary`,
          startsAt: '2026-10-03T19:00:00.000Z',
          endsAt: '2026-10-03T21:00:00.000Z',
          location: `${CANARY}-location`,
          address: `${CANARY}-address`,
          timezone: 'America/Los_Angeles',
          body: `${CANARY}-body`,
          capacity: 12,
          colour: 'plum',
          posterUrl: null,
          revealGuests: false,
        },
        0,
      );

      const storedEvent = await tx.execute<Record<string, unknown>>(
        sql`select * from event where id = ${row.id}`,
      );
      const storedObject = await tx.execute<Record<string, unknown>>(sql`
        select * from personal_protected_object
        where application_id = 'ronitnath-events'`);
      const atRest = JSON.stringify([storedEvent.rows, storedObject.rows]);
      if (atRest.includes(CANARY)) throw new Error('canary plaintext reached PostgreSQL');

      const opened = await hostedEvent(tx, { personId, isOperator: false }, row.id);
      if (opened?.title !== CANARY || opened.body !== `${CANARY}-body`) {
        throw new Error('protected event did not round trip');
      }
      const operator = await hostedEvent(tx, { personId: personId + 1, isOperator: true }, row.id);
      if (operator !== null) throw new Error('operator bypass opened personal event content');
      passed = true;
      throw new CanaryComplete('rollback qualification fixture');
    });
  } catch (error) {
    if (!(error instanceof CanaryComplete)) throw error;
  }
  if (!passed) throw new Error('event privacy qualification did not finish');
  console.log(
    JSON.stringify({
      passed: true,
      fixtureRolledBack: true,
      plaintextAbsentFromSql: true,
      decryptRoundTrip: true,
      operatorBypassDenied: true,
    }),
  );
}

main()
  .catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  })
  .finally(() => pool().end());
