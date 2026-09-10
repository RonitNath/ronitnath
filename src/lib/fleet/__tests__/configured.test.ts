/* Draft, publish, and the conflict. Against a real database because the whole
 * safety of the thing is one `ON CONFLICT ... WHERE version = ?` and one
 * `SELECT ... FOR UPDATE`: what is being asserted is that Postgres refuses the
 * second of two writers, not that this module remembered to check.
 *
 * Each run invents its own org id; configured rows are not append-only, but
 * leaving them costs nothing and a delete would race a parallel run. */

import { and, eq } from 'drizzle-orm';
import { afterAll, describe, expect, it, vi } from 'vitest';
import { z } from 'zod';

vi.mock('next/headers', () => ({ headers: async () => new Headers() }));

import { anOrg, closeDatabase, database, reachable } from './db';
import { schema } from '@/db/client';
import {
  ConfiguredConflict,
  publish,
  readDraft,
  readPublished,
  saveDraft,
} from '@/lib/fleet/configured';
import { eventsSince } from '@/lib/fleet/events';

afterAll(closeDatabase);

const hours = z.object({ open: z.string(), close: z.string() });

describe.skipIf(!reachable)('configured', () => {
  it('versions a draft from nothing and reads it back', async () => {
    const db = database();
    const orgId = anOrg('draft');

    const first = await db.transaction((tx) =>
      saveDraft(tx, orgId, 'hours', { open: '09:00', close: '17:00' }, 0),
    );
    expect(first).toBe(1);

    const second = await db.transaction((tx) =>
      saveDraft(tx, orgId, 'hours', { open: '08:00', close: '17:00' }, 1),
    );
    expect(second).toBe(2);
    expect(await readDraft(db, orgId, 'hours')).toEqual({
      version: 2,
      body: { open: '08:00', close: '17:00' },
    });
  });

  it('refuses a save against a stale version and says what moved', async () => {
    const db = database();
    const orgId = anOrg('stale');

    await db.transaction((tx) =>
      saveDraft(tx, orgId, 'hours', { open: '09:00', close: '17:00' }, 0),
    );
    /* The other editor got there first. */
    await db.transaction((tx) =>
      saveDraft(tx, orgId, 'hours', { open: '10:00', close: '17:00' }, 1),
    );

    const conflict = await db
      .transaction((tx) => saveDraft(tx, orgId, 'hours', { open: '09:00', close: '18:00' }, 1))
      .catch((error: unknown) => error);

    expect(conflict).toBeInstanceOf(ConfiguredConflict);
    const thrown = conflict as InstanceType<typeof ConfiguredConflict>;
    expect(thrown.expectedVersion).toBe(1);
    expect(thrown.current?.version).toBe(2);
    expect(thrown.current?.body).toEqual({ open: '10:00', close: '17:00' });
    /* Both fields moved: `open` because the other editor changed it, `close`
     * because this one did. The interface shows them and a human chooses;
     * nothing here merges them. */
    expect(thrown.diff).toEqual({
      open: { current: '10:00', incoming: '09:00' },
      close: { current: '17:00', incoming: '18:00' },
    });

    /* And the losing write did not land. */
    expect(await readDraft(db, orgId, 'hours')).toEqual({
      version: 2,
      body: { open: '10:00', close: '17:00' },
    });
  });

  it('lets exactly one of two concurrent saves win', async () => {
    const db = database();
    const orgId = anOrg('race');
    await db.transaction((tx) => saveDraft(tx, orgId, 'hours', { open: '09:00' }, 0));

    const results = await Promise.allSettled([
      db.transaction((tx) => saveDraft(tx, orgId, 'hours', { open: 'a' }, 1)),
      db.transaction((tx) => saveDraft(tx, orgId, 'hours', { open: 'b' }, 1)),
    ]);

    const won = results.filter((r) => r.status === 'fulfilled');
    const lost = results.filter((r) => r.status === 'rejected');
    expect(won).toHaveLength(1);
    expect(lost).toHaveLength(1);
    expect((lost[0] as PromiseRejectedResult).reason).toBeInstanceOf(ConfiguredConflict);
  });

  it('publishes the draft, records the version and emits', async () => {
    const db = database();
    const orgId = anOrg('publish');

    await db.transaction((tx) =>
      saveDraft(tx, orgId, 'hours', { open: '09:00', close: '17:00' }, 0),
    );
    const live = await db.transaction((tx) => publish(tx, orgId, 'hours', 1));
    expect(live).toBe(1);

    expect(await readPublished(db, orgId, 'hours', hours)).toEqual({
      open: '09:00',
      close: '17:00',
    });

    const versions = await db
      .select()
      .from(schema.configuredVersion)
      .where(and(eq(schema.configuredVersion.orgId, orgId), eq(schema.configuredVersion.key, 'hours')));
    expect(versions).toHaveLength(1);
    expect(versions[0]?.version).toBe(1);

    const [event] = await eventsSince(db, orgId, 0, 10);
    expect(event?.resourceKind).toBe('configured');
    expect(event?.resourceId).toBe('hours');
    expect(event?.kind).toBe('published');
    expect(event?.payload).toEqual({ version: 1 });
  });

  it('refuses to publish a version that is no longer the draft', async () => {
    const db = database();
    const orgId = anOrg('publish-stale');

    await db.transaction((tx) => saveDraft(tx, orgId, 'hours', { open: '09:00' }, 0));
    await db.transaction((tx) => saveDraft(tx, orgId, 'hours', { open: '10:00' }, 1));

    await expect(db.transaction((tx) => publish(tx, orgId, 'hours', 1))).rejects.toBeInstanceOf(
      ConfiguredConflict,
    );
    /* Nothing was published, so the site is still showing nothing. */
    expect(await readPublished(db, orgId, 'hours', hours)).toBeNull();
  });

  it('does not show a draft to a reader', async () => {
    const db = database();
    const orgId = anOrg('unpublished');
    await db.transaction((tx) => saveDraft(tx, orgId, 'hours', { open: '09:00' }, 0));
    expect(await readPublished(db, orgId, 'hours', hours)).toBeNull();
  });

  it('parses the published body, and complains when it no longer fits', async () => {
    const db = database();
    const orgId = anOrg('shape');
    await db.transaction((tx) => saveDraft(tx, orgId, 'hours', { opens: '09:00' }, 0));
    await db.transaction((tx) => publish(tx, orgId, 'hours', 1));
    await expect(readPublished(db, orgId, 'hours', hours)).rejects.toThrow();
  });
});
