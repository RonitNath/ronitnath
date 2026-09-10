/* What the deployment says about itself, and who is allowed to ask.
 *
 * Every number here is a number the process just measured or just counted —
 * the round-trip is timed around one `select 1`, the migrations are the rows
 * drizzle wrote when it applied them. Nothing is extrapolated into a rate
 * nobody observed. */

import { count, eq, isNull, sql } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import { OPERATOR_RESOURCE } from '@/features/auth/principal';
import { encodeId } from '@/lib/ids';

import journal from '../../../drizzle/meta/_journal.json';

export interface DeploymentReport {
  version: string;
  latencyMs: number;
  counts: { persons: number; organizations: number; events: number; sessions: number; audit: number };
  migrations: { tag: string; appliedAt: Date | null }[];
}

export async function deploymentReport(): Promise<DeploymentReport> {
  const db = database();
  const started = performance.now();
  await db.execute(sql`select 1`);
  const latencyMs = Math.round((performance.now() - started) * 100) / 100;

  const [persons] = await db
    .select({ n: count() })
    .from(schema.person)
    .where(isNull(schema.person.mergedInto));
  const [organizations] = await db.select({ n: count() }).from(schema.organization);
  const [events] = await db.select({ n: count() }).from(schema.event);
  const [sessions] = await db
    .select({ n: count() })
    .from(schema.session)
    .where(isNull(schema.session.revokedAt));
  const [audit] = await db.select({ n: count() }).from(schema.audit);

  /* drizzle-kit records what it applied in its own schema, keyed by the hash
   * of the file; the journal in the tree is what names them. Ordering is the
   * only thing that pairs the two, and it is the same ordering that applied
   * them. */
  let applied: { created_at: unknown }[] = [];
  try {
    const result = await db.execute(
      sql`select created_at from drizzle.__drizzle_migrations order by created_at`,
    );
    applied = result.rows as { created_at: unknown }[];
  } catch {
    /* a database that has never been migrated has no such table */
  }

  return {
    version: process.env.APP_VERSION ?? 'dev',
    latencyMs,
    counts: {
      persons: persons?.n ?? 0,
      organizations: organizations?.n ?? 0,
      events: events?.n ?? 0,
      sessions: sessions?.n ?? 0,
      audit: audit?.n ?? 0,
    },
    migrations: journal.entries.map((entry, index) => ({
      tag: entry.tag,
      appliedAt: applied[index] ? new Date(Number(applied[index].created_at)) : null,
    })),
  };
}

export interface OperatorRow {
  publicId: string;
  name: string;
  since: Date;
}

/** Everyone holding `operator` on `platform:*`. */
export async function listOperators(): Promise<OperatorRow[]> {
  const rows = await database()
    .select({
      id: schema.relation.subjectId,
      name: schema.person.displayName,
      since: schema.relation.createdAt,
    })
    .from(schema.relation)
    .innerJoin(schema.person, eq(schema.person.id, schema.relation.subjectId))
    .where(
      sql`${schema.relation.subjectKind} = 'person' and ${schema.relation.verb} = 'operator'
          and ${schema.relation.resourceKind} = ${OPERATOR_RESOURCE.kind}
          and ${schema.relation.resourceId} = ${OPERATOR_RESOURCE.id}`,
    )
    .orderBy(schema.relation.createdAt);
  return rows.map((row) => ({
    publicId: encodeId('person', row.id),
    name: row.name,
    since: row.since,
  }));
}
