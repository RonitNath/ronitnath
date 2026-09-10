/* The database the fleet suites run against, and the one thing that has to be
 * done differently here than in the app.
 *
 * `src/db/client.ts` builds its Drizzle instance from
 * `{ ...kernel, auth: authSchema }`, where `authSchema` is an
 * `import * as` namespace. Next's bundler turns that into an ordinary object,
 * but under Vite it stays a real module namespace — and a module namespace has
 * a null prototype, which Drizzle's `is()` walks into while it builds the
 * relational config: `Object.getPrototypeOf(ns).constructor` throws before any
 * connection is attempted. Every database-backed suite in this repo therefore
 * reports "no database listening" and silently skips, whether one is running
 * or not. (That is worth fixing in `src/db/client.ts` — one spread of the
 * namespace — but that file belongs to another leg.)
 *
 * So the harness builds the instance itself, with the namespace flattened, and
 * parks it where `client.ts` keeps its own: `database()` and `pool()` then
 * hand the app's own accessors back to production code unchanged, and the
 * modules under test see exactly what they see in a server.
 *
 * The DSN is the dev database with the spine migrations applied. Override with
 * `FLEET_TEST_DATABASE_URL`. */

import { sql } from 'drizzle-orm';
import { drizzle } from 'drizzle-orm/node-postgres';
import { Pool } from 'pg';

import * as authSchema from '@/db/auth-schema';
import * as kernel from '@/db/schema';
import { database, pool } from '@/db/client';

export const TEST_DSN =
  process.env.FLEET_TEST_DATABASE_URL ?? 'postgres://ronitnath:ronitnath@127.0.0.1:5443/rn_fresh';

/* `stream.ts` opens its own LISTEN connection from the environment, so this
 * has to be the environment and not only the pool. */
process.env.DATABASE_URL = TEST_DSN;

const globalForDb = globalThis as unknown as { rnPool?: Pool; rnDb?: unknown };

if (!globalForDb.rnPool) {
  const created = new Pool({ connectionString: TEST_DSN, max: 10 });
  globalForDb.rnPool = created;
  globalForDb.rnDb = drizzle(created, { schema: { ...kernel, auth: { ...authSchema } } });
}

/** Whether the spine is there to be tested. A suite that cannot reach it skips
 *  rather than fails: the gate has to be green on a machine with no database
 *  running, and a red test that means "you did not start Postgres" teaches
 *  everyone to ignore red tests. */
export const reachable = await (async () => {
  try {
    await database().execute(sql`select 1 from domain_event limit 1`);
    return true;
  } catch {
    return false;
  }
})();

export async function closeDatabase(): Promise<void> {
  if (globalForDb.rnPool) {
    await pool().end();
    delete globalForDb.rnPool;
    delete globalForDb.rnDb;
  }
}

/** A fresh org id per run. `domain_event` has a BEFORE DELETE trigger that
 *  raises — an append-only log a test can empty is not append-only — so suites
 *  do not clean up after themselves; they assert about an org nobody else
 *  used. */
export function anOrg(label: string): string {
  return `t_${label}_${Date.now().toString(36)}_${Math.floor(Math.random() * 1e6).toString(36)}`;
}

export { database, pool };
