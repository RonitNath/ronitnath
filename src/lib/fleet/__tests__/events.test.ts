/* The spine against a real database, because the property under test lives in
 * a trigger and not in this code. `seq` is gapless per org only if the BEFORE
 * INSERT trigger's cursor upsert serialises concurrent inserts; a fake would
 * be testing the fake, and the failure this guards against — two transactions
 * handed the same sequence, or a hole where one rolled back — only appears
 * under concurrency.
 *
 * The suite skips itself when no database is listening, so the gate stays
 * green on a machine with nothing running.
 *
 * Nothing is cleaned up afterwards, and that is not laziness: `domain_event`
 * has a BEFORE DELETE trigger that raises, because an append-only log that a
 * test suite can empty is not append-only. Each run therefore invents its own
 * org id and asserts only about that org. */

import { sql } from 'drizzle-orm';
import { afterAll, describe, expect, it, vi } from 'vitest';

vi.mock('next/headers', () => ({ headers: async () => new Headers() }));

import { anOrg, closeDatabase, database, reachable } from './db';
import { setRequestContext, withCorrelation } from '@/lib/fleet/context';
import { emit, eventsSince, latestSeq } from '@/lib/fleet/events';

afterAll(closeDatabase);

describe.skipIf(!reachable)('emit', () => {
  it('assigns a gapless sequence under concurrent transactions', async () => {
    const db = database();
    const orgId = anOrg('gapless');
    const count = 24;

    const assigned = await Promise.all(
      Array.from({ length: count }, (_, i) =>
        db.transaction((tx) =>
          emit(tx, {
            orgId,
            resourceKind: 'event',
            resourceId: `e_${i}`,
            kind: 'created',
            payload: { i },
          }),
        ),
      ),
    );

    /* Every transaction got its own number, and the numbers are 1..count with
     * nothing missing. Either half failing is the same bug seen from a
     * different side: a duplicate means the lock did not hold, a gap means a
     * sequence was claimed by a transaction that is not in the log. */
    expect([...assigned].sort((a, b) => a - b)).toEqual(
      Array.from({ length: count }, (_, i) => i + 1),
    );

    const rows = await eventsSince(db, orgId, 0, count + 10);
    expect(rows.map((row) => row.seq)).toEqual(Array.from({ length: count }, (_, i) => i + 1));
    expect(await latestSeq(db, orgId)).toBe(count);
  });

  it('leaves a rolled-back transaction out of the log', async () => {
    const db = database();
    const orgId = anOrg('rollback');

    await db.transaction(async (tx) => {
      await emit(tx, { orgId, resourceKind: 'event', resourceId: 'e_1', kind: 'created' });
    });
    await expect(
      db.transaction(async (tx) => {
        await emit(tx, { orgId, resourceKind: 'event', resourceId: 'e_2', kind: 'created' });
        throw new Error('deliberate');
      }),
    ).rejects.toThrow('deliberate');
    await db.transaction(async (tx) => {
      await emit(tx, { orgId, resourceKind: 'event', resourceId: 'e_3', kind: 'created' });
    });

    /* The cursor moved for the rolled-back attempt and then rolled back with
     * it, so the surviving rows are still 1 and 2 — the log has no hole even
     * though a sequence was handed out and thrown away. */
    const rows = await eventsSince(db, orgId, 0, 10);
    expect(rows.map((row) => [row.resourceId, row.seq])).toEqual([
      ['e_1', 1],
      ['e_3', 2],
    ]);
  });

  it('fills actor, subject, operator and correlation from the request context', async () => {
    const db = database();
    const orgId = anOrg('context');

    await withCorrelation('corr-emit', async () => {
      setRequestContext({
        actorId: 'p_actor',
        subjectId: 'p_subject',
        actingOperatorId: 'p_operator',
      });
      await db.transaction((tx) =>
        emit(tx, {
          orgId,
          resourceKind: 'rsvp',
          resourceId: 'r_1',
          kind: 'answered',
          published: false,
          payload: { answer: 'yes' },
        }),
      );
    });

    const [row] = await eventsSince(db, orgId, 0, 10);
    expect(row).toBeDefined();
    expect(row?.actorId).toBe('p_actor');
    expect(row?.subjectId).toBe('p_subject');
    expect(row?.actingOperatorId).toBe('p_operator');
    expect(row?.correlationId).toBe('corr-emit');
    expect(row?.published).toBe(false);
    expect(row?.payload).toEqual({ answer: 'yes' });
  });

  it('defaults to published with no payload', async () => {
    const db = database();
    const orgId = anOrg('defaults');
    await db.transaction((tx) =>
      emit(tx, { orgId, resourceKind: 'document', resourceId: 'r_9', kind: 'updated' }),
    );
    const [row] = await eventsSince(db, orgId, 0, 10);
    expect(row?.published).toBe(true);
    expect(row?.payload).toBeNull();
  });

  it('reports 0 as the latest sequence for an org that has never emitted', async () => {
    expect(await latestSeq(database(), anOrg('silent'))).toBe(0);
  });

  it('refuses to let anything rewrite the log', async () => {
    const db = database();
    const orgId = anOrg('append');
    await db.transaction((tx) =>
      emit(tx, { orgId, resourceKind: 'event', resourceId: 'e_1', kind: 'created' }),
    );
    /* Drizzle wraps a failed query and puts the database's own complaint on
     * `cause`, which is where the trigger's words are. */
    const refusal = await db
      .execute(sql`delete from domain_event where org_id = ${orgId}`)
      .then(() => null)
      .catch((error: unknown) => error);
    expect(refusal).not.toBeNull();
    expect(String((refusal as { cause?: unknown }).cause ?? refusal)).toMatch(/append-only/);
  });
});
