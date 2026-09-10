/* Configured records: anything with a publish boundary (fleet-conventions §7,
 * class 2). Prompts, hours, catalogues, brand, flags — state that an editor
 * changes deliberately and that everybody else should only see once it has
 * been published.
 *
 * Two rows under one key. The draft is what the editor is working on; the
 * published row is what the site reads. `readPublished` never looks at the
 * draft, so an unfinished edit cannot reach a visitor by accident, and
 * `publish` is the only thing that moves one to the other. A version history
 * accumulates beside them: every publish appends the body it published, so
 * "what did this say in August" is a row and not an archaeology exercise.
 *
 * Every write carries the version it believed it was editing, and a mismatch
 * is a conflict — never a merge. Two editors on one prompt is not a text
 * document with a shared history; it is two people who each think they know
 * what the assistant is about to say, and silently interleaving their edits
 * produces a body neither of them wrote and neither of them will recognise.
 * `ConfiguredConflict` carries the current row and a field-level diff so that
 * the interface can show what changed underneath and let a human choose.
 *
 * `version` counts drafts, not publishes: it is bumped on every save so that
 * two saves against the same base cannot both succeed, and `publish` records
 * the draft version it froze. A key can therefore publish version 1 and then
 * version 7 — the gaps are the saves that were never published, which is the
 * honest history. */

import { and, eq, sql } from 'drizzle-orm';
import type { ZodType } from 'zod';

import { schema } from '@/db/client';
import { requestContext } from './context';
import { emit, type Queryable, type Transaction } from './events';

/** The resource kind every configured event is emitted under, so a watcher can
 *  ask for configuration changes without enumerating keys. */
export const CONFIGURED_KIND = 'configured';

export type ConfiguredState = 'draft' | 'published';

export interface ConfiguredRow {
  version: number;
  body: unknown;
  updatedBy: string | null;
  updatedAt: Date;
}

/** One field that moved between what the caller edited and what is stored.
 *  A field the other editor removed reads as `current: undefined`. */
export interface FieldDiff {
  current: unknown;
  incoming: unknown;
}

/** Thrown when a write's `expectedVersion` is not the version in the database.
 *  The caller answers 409 with `current` and `diff`; it must not retry with the
 *  new version, because that is a merge with extra steps. */
export class ConfiguredConflict extends Error {
  readonly current: ConfiguredRow | null;
  readonly diff: Record<string, FieldDiff>;

  constructor(
    readonly key: string,
    readonly expectedVersion: number,
    current: ConfiguredRow | null,
    diff: Record<string, FieldDiff>,
  ) {
    super(
      `configured "${key}" has moved on: expected version ${expectedVersion}, found ${
        current?.version ?? 'no row'
      }`,
    );
    this.name = 'ConfiguredConflict';
    this.current = current;
    this.diff = diff;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/* A field-level diff over the top level and no deeper. The interface showing
 * a conflict wants to say "they changed the greeting, you changed the hours";
 * a recursive diff of a prompt body says "they changed something at
 * blocks[3].text.spans[0]", which is true and useless. Bodies that are not
 * objects — a string, an array of rules — compare whole. */
function diffBodies(current: unknown, incoming: unknown): Record<string, FieldDiff> {
  if (!isRecord(current) || !isRecord(incoming)) {
    const same = JSON.stringify(current) === JSON.stringify(incoming);
    return same ? {} : { '': { current, incoming } };
  }

  const diff: Record<string, FieldDiff> = {};
  for (const field of new Set([...Object.keys(current), ...Object.keys(incoming)])) {
    if (JSON.stringify(current[field]) !== JSON.stringify(incoming[field])) {
      diff[field] = { current: current[field], incoming: incoming[field] };
    }
  }
  return diff;
}

async function read(
  db: Queryable,
  orgId: string,
  key: string,
  state: ConfiguredState,
): Promise<ConfiguredRow | null> {
  const rows = await db
    .select({
      version: schema.configured.version,
      body: schema.configured.body,
      updatedBy: schema.configured.updatedBy,
      updatedAt: schema.configured.updatedAt,
    })
    .from(schema.configured)
    .where(
      and(
        eq(schema.configured.orgId, orgId),
        eq(schema.configured.key, key),
        eq(schema.configured.state, state),
      ),
    )
    .limit(1);
  return rows[0] ?? null;
}

/** Save the draft under `key`. `expectedVersion` is the version the editor
 *  loaded — 0 for a key that has never been saved. Returns the new version. */
export async function saveDraft(
  tx: Transaction,
  orgId: string,
  key: string,
  body: unknown,
  expectedVersion: number,
): Promise<number> {
  const ctx = await requestContext();

  /* One statement, so that two saves against the same base cannot both pass
   * their check before either writes. The conflict clause only fires when the
   * stored version is the one the caller expected; otherwise Postgres does
   * nothing and returns no row, which is how we learn we lost. */
  const saved = await tx
    .insert(schema.configured)
    .values({
      orgId,
      key,
      state: 'draft',
      version: expectedVersion + 1,
      body,
      updatedBy: ctx.actorId,
    })
    .onConflictDoUpdate({
      target: [schema.configured.orgId, schema.configured.key, schema.configured.state],
      setWhere: eq(schema.configured.version, expectedVersion),
      set: {
        version: sql`${schema.configured.version} + 1`,
        body,
        updatedBy: ctx.actorId,
        updatedAt: new Date(),
      },
    })
    .returning({ version: schema.configured.version });

  const row = saved[0];
  if (row) return row.version;

  const current = await read(tx, orgId, key, 'draft');
  throw new ConfiguredConflict(key, expectedVersion, current, diffBodies(current?.body, body));
}

/** Publish the draft under `key`. Copies draft → published, appends a version
 *  row, and emits `kind: 'published'` so every watcher refreshes. Returns the
 *  version that is now live. */
export async function publish(
  tx: Transaction,
  orgId: string,
  key: string,
  expectedDraftVersion: number,
): Promise<number> {
  const ctx = await requestContext();

  /* The draft is locked for the rest of the transaction: publishing reads it,
   * writes it three places and emits, and a save landing in the middle of that
   * would put a body in `configured_version` that was never the draft anyone
   * approved. */
  const locked = await tx
    .select({ version: schema.configured.version, body: schema.configured.body })
    .from(schema.configured)
    .where(
      and(
        eq(schema.configured.orgId, orgId),
        eq(schema.configured.key, key),
        eq(schema.configured.state, 'draft'),
      ),
    )
    .limit(1)
    .for('update');

  const draft = locked[0];
  if (!draft || draft.version !== expectedDraftVersion) {
    const current = await read(tx, orgId, key, 'draft');
    throw new ConfiguredConflict(key, expectedDraftVersion, current, {});
  }

  await tx
    .insert(schema.configured)
    .values({
      orgId,
      key,
      state: 'published',
      version: draft.version,
      body: draft.body,
      updatedBy: ctx.actorId,
    })
    .onConflictDoUpdate({
      target: [schema.configured.orgId, schema.configured.key, schema.configured.state],
      set: {
        version: draft.version,
        body: draft.body,
        updatedBy: ctx.actorId,
        updatedAt: new Date(),
      },
    });

  /* Republishing a version that is already live is a no-op on the history
   * rather than an error: the body under (org, key, version) is by definition
   * the same body. */
  await tx
    .insert(schema.configuredVersion)
    .values({
      orgId,
      key,
      version: draft.version,
      body: draft.body,
      publishedBy: ctx.actorId,
    })
    .onConflictDoNothing();

  await emit(tx, {
    orgId,
    resourceKind: CONFIGURED_KIND,
    resourceId: key,
    kind: 'published',
    payload: { version: draft.version },
  });

  return draft.version;
}

/** The live body under `key`, parsed. Null when the key has never been
 *  published — the caller supplies the default, because a default that lives
 *  here would be a second source of truth for the same setting.
 *
 *  Parsing is not optional. The body is jsonb written by an older version of
 *  this code, and the schema is the only thing standing between a renamed
 *  field and a page that renders `undefined`. */
export async function readPublished<T>(
  db: Queryable,
  orgId: string,
  key: string,
  zodSchema: ZodType<T>,
): Promise<T | null> {
  const row = await read(db, orgId, key, 'published');
  return row ? zodSchema.parse(row.body) : null;
}

/** The draft body under `key` with its version — what an editor loads and
 *  hands back to {@link saveDraft}. Version 0 means there is no draft yet. */
export async function readDraft(
  db: Queryable,
  orgId: string,
  key: string,
): Promise<{ version: number; body: unknown }> {
  const row = await read(db, orgId, key, 'draft');
  return { version: row?.version ?? 0, body: row?.body ?? null };
}
