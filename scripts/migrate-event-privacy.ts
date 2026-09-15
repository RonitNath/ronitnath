import { and, eq, sql } from 'drizzle-orm';

import { database, pool, schema } from '../src/db/client';
import {
  eventContentFromLegacy,
  ownerScope,
  writeEventContent,
  type EventRouting,
} from '../src/features/privacy/event-content';

const limit = Math.max(1, Math.min(1_000, Number(process.env.PRIVACY_MIGRATION_LIMIT ?? 100)));

async function migrateOne(): Promise<number | null> {
  return database().transaction(async (tx) => {
    const rows = await tx
      .select()
      .from(schema.event)
      .where(eq(schema.event.privacyRevision, 0))
      .orderBy(schema.event.id)
      .limit(1)
      .for('update', { skipLocked: true });
    const row = rows[0];
    if (!row) return null;

    const routing: EventRouting = {
      id: row.id,
      hostPersonId: row.hostPersonId,
      slug: row.slug,
      sequence: row.sequence,
      privacyRevision: 1,
      publishedAt: row.publishedAt,
      updatedAt: row.updatedAt,
      createdAt: row.createdAt,
    };
    await writeEventContent(tx, routing, eventContentFromLegacy(row), 0);
    const changed = await tx
      .update(schema.event)
      .set({
        title: null,
        summary: null,
        startsAt: null,
        endsAt: null,
        location: null,
        address: null,
        timezone: null,
        body: null,
        capacity: null,
        colour: null,
        posterUrl: null,
        revealGuests: null,
        privacyRevision: 1,
        updatedAt: sql`now()`,
      })
      .where(and(eq(schema.event.id, row.id), eq(schema.event.privacyRevision, 0)))
      .returning({ id: schema.event.id });
    if (!changed[0]) throw new Error(`event ${row.id} changed during privacy migration`);
    await tx.execute(sql`
      update personal_privacy_scope
      set migrated_objects = migrated_objects + 1,
          migration_cursor = ${String(row.id)},
          lifecycle = case
            when exists (
              select 1 from event
              where host_person_id = ${row.hostPersonId} and privacy_revision = 0
            ) then 'migrating'
            else 'protected'
          end,
          updated_at = now()
      where application_id = 'ronitnath-events'
        and owner_scope = ${ownerScope(row.hostPersonId)}`);
    return row.id;
  });
}

async function main(): Promise<void> {
  let migrated = 0;
  let last: number | null = null;
  while (migrated < limit) {
    last = await migrateOne();
    if (last === null) break;
    migrated += 1;
  }
  const remaining = await database().execute<{ count: string }>(
    sql`select count(*)::text as count from event where privacy_revision = 0`,
  );
  console.log(
    JSON.stringify({
      migrated,
      lastEventId: last,
      remaining: Number(remaining.rows[0]?.count ?? 0),
      resumable: true,
    }),
  );
}

main()
  .catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  })
  .finally(() => pool().end());
