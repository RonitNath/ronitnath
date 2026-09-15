/* The migration runner. `drizzle-kit migrate` reads the same journal, but it
 * is a development tool: it prints a spinner, honours `strict`, and on this
 * repo it can decide to do nothing without saying so. A deploy needs the
 * opposite — every statement named, a non-zero exit on the first failure, and
 * a line at the end saying which migration the database is now at. */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { drizzle } from 'drizzle-orm/node-postgres';
import type { NodePgDatabase } from 'drizzle-orm/node-postgres';
import { migrate } from 'drizzle-orm/node-postgres/migrator';
import { sql } from 'drizzle-orm';
import { Pool } from 'pg';
import { withMigrationLock } from '@isoastra/fleet-delivery/postgres';
import { z } from 'zod';

const folder = join(process.cwd(), 'drizzle');

interface Journal {
  entries: { idx: number; tag: string }[];
}

const executionPolicySchema = z
  .object({
    schemaVersion: z.literal(2),
    entries: z.array(
      z
        .object({
          name: z.string().min(1),
          mode: z.enum(['expand', 'transition', 'contract']),
          transactional: z.boolean(),
          lockTimeoutMs: z.number().int().positive(),
          statementTimeoutMs: z.number().int().positive(),
          backfill: z
            .object({
              required: z.boolean(),
              complete: z.boolean(),
              probe: z.string().min(1).nullable(),
            })
            .strict(),
        })
        .strict(),
    ),
  })
  .strict();

async function main(): Promise<void> {
  const connectionString = process.env.MIGRATION_DATABASE_URL ?? process.env.DATABASE_URL;
  if (!connectionString) throw new Error('MIGRATION_DATABASE_URL or DATABASE_URL is not set');

  const journal: Journal = JSON.parse(
    readFileSync(join(folder, 'meta', '_journal.json'), 'utf8'),
  ) as Journal;
  const last = journal.entries.at(-1);
  if (!last) throw new Error('no migrations in drizzle/meta/_journal.json');

  /* One dedicated advisory-lock connection plus the migrator connection. */
  const pool = new Pool({ connectionString, max: 2 });
  try {
    const lock = await pool.connect();
    try {
      const policy = executionPolicySchema.parse(
        JSON.parse(readFileSync(join(process.cwd(), 'delivery.migrations.json'), 'utf8')),
      );
      for (const entry of policy.entries) {
        if (!journal.entries.some(({ tag }) => tag === entry.name)) {
          throw new Error(`migration policy names unknown migration ${entry.name}`);
        }
        if (entry.mode === 'contract' && entry.backfill.required && !entry.backfill.complete) {
          throw new Error(`contract migration ${entry.name} requires a completed backfill`);
        }
      }
      const limits = {
        lockTimeoutMs: Math.min(...policy.entries.map((entry) => entry.lockTimeoutMs)),
        statementTimeoutMs: Math.min(
          ...policy.entries.map((entry) => entry.statementTimeoutMs),
        ),
      };
      await withMigrationLock(lock, 7403140, limits, async (lockedClient) => {
        const lockedDb = drizzle(lockedClient);
        await verifyHistory(lockedDb, journal);
        const before = await applied(lockedDb);
        if (process.env.MIGRATION_PROBE_ONLY === '1') {
          if (before !== journal.entries.length)
            throw new Error(
              `migration probe found ${before} of ${journal.entries.length} entries`,
            );
          console.log(
            `migrate: probe complete, ${before} of ${journal.entries.length} total, at ${last.tag}`,
          );
          return;
        }
        await migrate(lockedDb, { migrationsFolder: folder });
        const after = await applied(lockedDb);
        console.log(
          `migrate: ${after - before} applied, ${after} of ${journal.entries.length} total, at ${last.tag}`,
        );
        if (after !== journal.entries.length)
          throw new Error(
            `expected ${journal.entries.length} migrations to be recorded, found ${after}`,
          );
        await verifyHistory(lockedDb, journal);
      });
    } finally {
      lock.release();
    }
  } finally {
    await pool.end();
  }
}

async function verifyHistory(db: NodePgDatabase, journal: Journal): Promise<void> {
  const exists = await db.execute<{ n: string }>(sql`
    select count(*)::text as n from information_schema.tables
    where table_schema = 'drizzle' and table_name = '__drizzle_migrations'`);
  if (exists.rows[0]?.n === '0') return;
  const recorded = await db.execute<{ hash: string; created_at: string }>(sql`
    select hash, created_at::text from drizzle.__drizzle_migrations order by created_at`);
  if (recorded.rows.length > journal.entries.length)
    throw new Error('database migration history is ahead of this artifact');
  for (const [index, row] of recorded.rows.entries()) {
    const entry = journal.entries[index];
    if (!entry) throw new Error(`migration history has an unknown entry at index ${index}`);
    const expected = createHash('sha256')
      .update(readFileSync(join(folder, `${entry.tag}.sql`)))
      .digest('hex');
    if (row.hash !== expected) throw new Error(`migration checksum drift at ${entry.tag}`);
  }
}

async function applied(db: NodePgDatabase): Promise<number> {
  const rows = await db.execute<{ n: string }>(sql`
    select count(*)::text as n from information_schema.tables
    where table_schema = 'drizzle' and table_name = '__drizzle_migrations'`);
  if (rows.rows[0]?.n === '0') return 0;
  const count = await db.execute<{ n: string }>(
    sql`select count(*)::text as n from drizzle.__drizzle_migrations`,
  );
  return Number(count.rows[0]?.n ?? 0);
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
