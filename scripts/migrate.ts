/* The migration runner. `drizzle-kit migrate` reads the same journal, but it
 * is a development tool: it prints a spinner, honours `strict`, and on this
 * repo it can decide to do nothing without saying so. A deploy needs the
 * opposite — every statement named, a non-zero exit on the first failure, and
 * a line at the end saying which migration the database is now at. */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { drizzle } from 'drizzle-orm/node-postgres';
import { migrate } from 'drizzle-orm/node-postgres/migrator';
import { sql } from 'drizzle-orm';
import { Pool } from 'pg';
import { withMigrationLock } from '@isoastra/fleet-delivery/postgres';

const folder = join(process.cwd(), 'drizzle');

interface Journal {
  entries: { idx: number; tag: string }[];
}

async function main(): Promise<void> {
  const connectionString = process.env.DATABASE_URL;
  if (!connectionString) throw new Error('DATABASE_URL is not set');

  const journal: Journal = JSON.parse(
    readFileSync(join(folder, 'meta', '_journal.json'), 'utf8'),
  ) as Journal;
  const last = journal.entries.at(-1);
  if (!last) throw new Error('no migrations in drizzle/meta/_journal.json');

  /* One dedicated advisory-lock connection plus the migrator connection. */
  const pool = new Pool({ connectionString, max: 2 });
  const db = drizzle(pool);
  try {
    const before = await applied(db);
    const lock = await pool.connect();
    try {
      await withMigrationLock(lock, 7403140, {
        mode: 'expand',
        compatibleFrom: ['d2373d4823d43beb4a7d2b24741636ee7b449098'],
        lockTimeoutMs: 30_000,
        statementTimeoutMs: 120_000,
        transactional: true,
        backfillRequired: false,
      }, () => migrate(db, { migrationsFolder: folder }));
    } finally {
      lock.release();
    }
    const after = await applied(db);
    console.log(
      `migrate: ${after - before} applied, ${after} of ${journal.entries.length} total, at ${last.tag}`,
    );
    if (after !== journal.entries.length) {
      throw new Error(
        `expected ${journal.entries.length} migrations to be recorded, found ${after}`,
      );
    }
  } finally {
    await pool.end();
  }
}

async function applied(db: ReturnType<typeof drizzle>): Promise<number> {
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
