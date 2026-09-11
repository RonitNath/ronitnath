import { randomUUID } from 'node:crypto';
import { eq, sql } from 'drizzle-orm';
import { afterAll, describe, expect, it, vi } from 'vitest';

vi.mock('next/headers', () => ({ headers: async () => new Headers() }));

import { schema } from '@/db/client';
import { setRequestContext, withCorrelation } from '@/lib/fleet/context';
import { emit } from '@/lib/fleet/events';
import { anOrg, closeDatabase, database, pool, reachable } from '@/lib/fleet/__tests__/db';

afterAll(closeDatabase);

async function view(options: { expired?: boolean; old?: boolean } = {}) {
  const id = randomUUID();
  await database()
    .insert(schema.realtimeView)
    .values({
      id,
      visitorId: randomUUID(),
      tabId: randomUUID(),
      path: '/',
      title: 'test',
      leaseExpiresAt: new Date(Date.now() + (options.expired ? -1_000 : 60_000)),
      lastHeartbeatAt: new Date(Date.now() - (options.old ? 8 * 86_400_000 : 0)),
    });
  return id;
}

async function event(orgId: string, resourceKind: string, resourceId: string) {
  await withCorrelation(randomUUID(), async () => {
    setRequestContext({ actorId: 'test' });
    await database().transaction((tx) =>
      emit(tx, { orgId, resourceKind, resourceId, kind: 'updated' }),
    );
  });
}

describe.skipIf(!reachable)('realtime PostgreSQL authority', () => {
  it('selects displayed resources and query membership without trusting displayed rows', async () => {
    const resourceView = await view();
    const queryView = await view();
    const forgedView = await view();
    await database()
      .insert(schema.realtimeSubscription)
      .values([
        {
          viewId: resourceView,
          subscriptionKey: 'resource:document:r_shown',
          descriptor: { type: 'resource', resourceKind: 'document', resourceId: 'r_shown' },
        },
        {
          viewId: queryView,
          subscriptionKey: 'query:document:documents:test',
          descriptor: { type: 'query', resourceKind: 'document', queryKey: 'documents:test' },
        },
        {
          viewId: forgedView,
          subscriptionKey: 'resource:document:r_other',
          descriptor: { type: 'resource', resourceKind: 'document', resourceId: 'r_other' },
        },
      ]);
    await event(anOrg('membership'), 'document', 'r_shown');
    const deliveries = await database()
      .select()
      .from(schema.realtimeDelivery)
      .where(
        sql`${schema.realtimeDelivery.viewId} in (${resourceView}::uuid, ${queryView}::uuid, ${forgedView}::uuid)`,
      );
    expect(deliveries.map((row) => row.viewId).sort()).toEqual(
      [queryView, resourceView].sort(),
    );
    expect(deliveries.find((row) => row.viewId === queryView)?.reason).toContain(
      'membership of query',
    );
  });

  it('does not dispatch to an expired lease and removes metadata after seven days', async () => {
    const expired = await view({ expired: true });
    const old = await view({ old: true });
    await database()
      .insert(schema.realtimeSubscription)
      .values({
        viewId: expired,
        subscriptionKey: 'query:document:expired',
        descriptor: { type: 'query', resourceKind: 'document', queryKey: 'expired' },
      });
    await event(anOrg('lease'), 'document', 'r_new');
    expect(
      await database()
        .select()
        .from(schema.realtimeDelivery)
        .where(eq(schema.realtimeDelivery.viewId, expired)),
    ).toHaveLength(0);
    await database().execute(sql`select realtime_cleanup()`);
    expect(
      await database()
        .select()
        .from(schema.realtimeView)
        .where(eq(schema.realtimeView.id, old)),
    ).toHaveLength(0);
  });

  it('notifies a listener held by another database connection', async () => {
    const listener = await pool().connect();
    try {
      await listener.query('LISTEN realtime_delivery');
      const noticed = new Promise<string>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error('notification timed out')), 2_000);
        listener.once('notification', (message) => {
          clearTimeout(timer);
          resolve(message.payload ?? '');
        });
      });
      const orgId = anOrg('replica');
      await event(orgId, 'document', 'r_replica');
      await expect(noticed).resolves.toContain(`${orgId}|`);
    } finally {
      listener.release();
    }
  });

  it('binds received and applied acknowledgments to the visitor, view, delivery, and revision', async () => {
    const { acknowledgeView } = await import('../acknowledgment');
    const viewId = await view();
    const [owned] = await database()
      .select()
      .from(schema.realtimeView)
      .where(eq(schema.realtimeView.id, viewId));
    await database()
      .insert(schema.realtimeSubscription)
      .values({
        viewId,
        subscriptionKey: 'resource:document:r_ack',
        descriptor: { type: 'resource', resourceKind: 'document', resourceId: 'r_ack' },
      });
    await event(anOrg('ack'), 'document', 'r_ack');
    const [delivery] = await database()
      .select()
      .from(schema.realtimeDelivery)
      .where(eq(schema.realtimeDelivery.viewId, viewId));
    expect(
      await acknowledgeView(owned!.visitorId, viewId, {
        deliveryId: delivery!.id,
        revision: delivery!.revision,
        stage: 'received',
      }),
    ).toBe(true);
    expect(
      await acknowledgeView(owned!.visitorId, viewId, {
        deliveryId: delivery!.id,
        revision: delivery!.revision,
        stage: 'applied',
      }),
    ).toBe(true);
    const [acknowledged] = await database()
      .select()
      .from(schema.realtimeDelivery)
      .where(eq(schema.realtimeDelivery.id, delivery!.id));
    expect(acknowledged?.receivedAt).toBeInstanceOf(Date);
    expect(acknowledged?.appliedAt).toBeInstanceOf(Date);
    expect(
      await acknowledgeView(randomUUID(), viewId, {
        deliveryId: delivery!.id,
        revision: delivery!.revision,
        stage: 'received',
      }),
    ).toBe(false);
  });
});
