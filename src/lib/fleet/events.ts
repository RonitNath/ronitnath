/* The spine (fleet-conventions §7, contracts.md §domain_event).
 *
 * One append-only table, written inside every mutating transaction, read by
 * realtime, audit, notifications and metering alike. A command that forgets to
 * `emit` is invisible to all four, and no amount of later work recovers the
 * event it did not write — that discipline is the whole point and is the one
 * part of this design that cannot be retrofitted.
 *
 * `seq` is gapless per org and this module never sets it. A BEFORE INSERT
 * trigger takes a row lock on the org's cursor and assigns it, which is what
 * lets a reconnecting browser say `since=<seq>` and be certain it missed
 * nothing: two concurrent transactions serialise on that lock rather than
 * racing to the same number or leaving a hole where a rolled-back transaction
 * used to be. Because the column is NOT NULL with no default, the insert is
 * written as SQL rather than through Drizzle's builder — the builder would
 * insist we name a value for it, and naming a value is exactly the bug.
 *
 * `orgId` is a public id or a slug, never an integer: on this site a person's
 * own stream is as much a stream as an organization's, and both appear in a
 * URL (`/u/<user>/events`, `/o/<org>/events`).
 *
 * The payload is a courtesy, not a contract. Readers refetch through the
 * authorized read path; what the event says is *that* something changed. */

import { and, desc, eq, gt } from 'drizzle-orm';
import { emit as sharedEmit, withActorContext } from '@isoastra/fleet-events';

import { database, schema } from '@/db/client';
import { requestContext } from './context';

type Db = ReturnType<typeof database>;
/** A Drizzle transaction handle: what a command body is given. */
export type Transaction = Parameters<Parameters<Db['transaction']>[0]>[0];
/** Anything that can run a read — the pool-backed instance or a transaction. */
export type Queryable = Db | Transaction;

export interface EmitInput {
  orgId: string;
  resourceKind: string;
  resourceId: string;
  kind: string;
  /* False for a draft, a hidden row, an internal step: the client hook can be
   * told to refresh only on published changes, and the audit trail still keeps
   * every one of them. Default true. */
  published?: boolean;
  payload?: Record<string, unknown> | null;
}

export type DomainEventRow = typeof schema.domainEvent.$inferSelect;

/** Append one event inside a command's transaction. Returns the sequence the
 *  database assigned, which is what a caller hands a client as `since`. */
export async function emit(tx: Transaction, input: EmitInput): Promise<number> {
  const ctx = await requestContext();
  return withActorContext(ctx, () => sharedEmit(tx, input));
}

/** The org's events after `sinceSeq`, oldest first — the replay a stream sends
 *  on connect and the catch-up it sends after a gap. `limit` is the caller's
 *  budget: ask for one more than you will send to learn whether there are
 *  more. */
export function eventsSince(
  db: Queryable,
  orgId: string,
  sinceSeq: number,
  limit: number,
): Promise<DomainEventRow[]> {
  return db
    .select()
    .from(schema.domainEvent)
    .where(and(eq(schema.domainEvent.orgId, orgId), gt(schema.domainEvent.seq, sinceSeq)))
    .orderBy(schema.domainEvent.seq)
    .limit(limit);
}

/** The org's latest sequence, or 0 for an org that has never emitted. Read
 *  from the log rather than the cursor: the cursor is claimed by a transaction
 *  that may still roll back, and a client told about a seq that never lands
 *  waits forever for events it has already been promised. */
export async function latestSeq(db: Queryable, orgId: string): Promise<number> {
  const rows = await db
    .select({ seq: schema.domainEvent.seq })
    .from(schema.domainEvent)
    .where(eq(schema.domainEvent.orgId, orgId))
    .orderBy(desc(schema.domainEvent.seq))
    .limit(1);
  return rows[0]?.seq ?? 0;
}
