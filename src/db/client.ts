import { drizzle, type NodePgDatabase } from 'drizzle-orm/node-postgres';
import { Pool } from 'pg';
import * as authSchema from './auth-schema';
import * as kernel from './schema';

/* Two schema modules, one Drizzle instance. They are kept apart because
 * better-auth owns `auth.*` and this app owns the rest, and because both
 * modules would otherwise want the names `session` and `organization`. */
/* Both are spread rather than nested as namespaces: an `import * as` object has
 * a null prototype, and Drizzle's `is()` reads `Object.getPrototypeOf(v).constructor`
 * while it builds the relational config, so handing it a namespace throws before
 * a connection is ever opened. */
const schema = { ...kernel, auth: { ...authSchema } };

/* The pool is opened on first query, never at import: the build collects page
 * data by loading every route module, and a route module must not need a
 * database to exist. One pool per process — Next reloads modules in dev, so it
 * is parked on globalThis rather than opened again on every save. */

const globalForDb = globalThis as unknown as {
  rnPool?: Pool;
  rnDb?: NodePgDatabase<typeof schema>;
  rnPoolClosing?: Promise<void>;
  rnPoolClosed?: boolean;
};

export function pool(): Pool {
  if (globalForDb.rnPoolClosed) throw new Error('application database pool is closing');
  if (globalForDb.rnPool) return globalForDb.rnPool;
  const connectionString = process.env.DATABASE_URL;
  if (!connectionString) throw new Error('DATABASE_URL is not set');
  const created = new Pool({
    connectionString,
    max: process.env.RUNTIME_ROLE === 'worker' ? 4 : 10,
    idleTimeoutMillis: 30_000,
    connectionTimeoutMillis: 5_000,
    query_timeout: 15_000,
    statement_timeout: 15_000,
    lock_timeout: 5_000,
  });
  created.on('error', (error) => {
    console.error(JSON.stringify({ level: 'error', event: 'database.idle-error', message: error.message }));
  });
  globalForDb.rnPool = created;
  return created;
}

export async function closePool(): Promise<void> {
  if (globalForDb.rnPoolClosing) return globalForDb.rnPoolClosing;
  const current = globalForDb.rnPool;
  globalForDb.rnPoolClosed = true;
  globalForDb.rnPool = undefined;
  globalForDb.rnDb = undefined;
  globalForDb.rnPoolClosing = current ? current.end() : Promise.resolve();
  await globalForDb.rnPoolClosing;
}

export function database(): NodePgDatabase<typeof schema> {
  globalForDb.rnDb ??= drizzle(pool(), { schema });
  return globalForDb.rnDb;
}

export { schema, authSchema };
