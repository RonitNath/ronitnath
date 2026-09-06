import { drizzle, type NodePgDatabase } from 'drizzle-orm/node-postgres';
import { Pool } from 'pg';
import * as schema from './schema';

/* The pool is opened on first query, never at import: the build collects page
 * data by loading every route module, and a route module must not need a
 * database to exist. One pool per process — Next reloads modules in dev, so it
 * is parked on globalThis rather than opened again on every save. */

const globalForDb = globalThis as unknown as {
  rnPool?: Pool;
  rnDb?: NodePgDatabase<typeof schema>;
};

export function pool(): Pool {
  if (globalForDb.rnPool) return globalForDb.rnPool;
  const connectionString = process.env.DATABASE_URL;
  if (!connectionString) throw new Error('DATABASE_URL is not set');
  const created = new Pool({ connectionString, max: 10, idleTimeoutMillis: 30_000 });
  globalForDb.rnPool = created;
  return created;
}

export function database(): NodePgDatabase<typeof schema> {
  globalForDb.rnDb ??= drizzle(pool(), { schema });
  return globalForDb.rnDb;
}

export { schema };
