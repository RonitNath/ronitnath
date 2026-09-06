import type { NodePgDatabase } from 'drizzle-orm/node-postgres';
import type * as schema from '@/db/schema';

/* The handle a command body gets: the transaction, never the pool. Naming it
 * once keeps every command signature honest about running inside one. */
export type Transaction = Parameters<
  Parameters<NodePgDatabase<typeof schema>['transaction']>[0]
>[0];
