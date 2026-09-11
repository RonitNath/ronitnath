import { createHash, randomUUID } from 'node:crypto';
import { and, desc, eq, gt, inArray, isNotNull, sql } from 'drizzle-orm';
import {
  LEASE_MS,
  subscriptionKey,
  type RealtimeSubscription,
  type ViewRegistration,
  viewRegistrationSchema,
} from '@isoastra/fleet-events/presence';

import { database, pool, schema } from '@/db/client';
import { currentPrincipal } from '@/features/auth/principal';
import { allows } from '@/lib/authority';
import { encodeId, tryDecodeId } from '@/lib/ids';

export interface RegisteredView {
  viewId: string;
  subscriptions: RealtimeSubscription[];
}

function documentIdFromPath(path: string): number | null {
  const match = path.match(/^\/u\/[^/]+\/documents\/([^/]+)$/);
  return match?.[1] ? tryDecodeId('document', match[1]) : null;
}

/** Resolve requested dependency descriptors through the same access checks as
 * the page. The client reports observations; the server grants subscriptions. */
async function authorize(path: string, requested: RealtimeSubscription[]) {
  const principal = await currentPrincipal();
  const allowed = new Map<string, RealtimeSubscription>();
  const offer = (subscription: RealtimeSubscription) =>
    allowed.set(subscriptionKey(subscription), subscription);

  if (path === '/') {
    offer({ type: 'configuration', key: 'homepage', state: 'published' });
    offer({ type: 'configuration', key: 'announcement', state: 'live' });
  }
  const publicSlug = path.match(/^\/d\/([^/]+)$/)?.[1];
  if (publicSlug) {
    const rows = await database()
      .select({ id: schema.document.id })
      .from(schema.document)
      .where(
        and(
          eq(schema.document.slug, decodeURIComponent(publicSlug)),
          isNotNull(schema.document.publishedAt),
        ),
      )
      .limit(1);
    const publicId = rows[0] ? encodeId('document', rows[0].id) : null;
    for (const candidate of requested) {
      if (
        candidate.type === 'resource' &&
        candidate.resourceKind === 'document' &&
        candidate.resourceId === publicId
      )
        offer(candidate);
    }
  }
  if (principal && /^\/u\/[^/]+\/documents$/.test(path)) {
    offer({
      type: 'query',
      resourceKind: 'document',
      queryKey: `documents:${principal.personId}`,
    });
  }
  const documentId = principal ? documentIdFromPath(path) : null;
  if (principal && documentId !== null) {
    const ok = await database().transaction((tx) =>
      allows(
        tx,
        { personId: principal.personId, isOperator: principal.isOperator },
        { on: 'document', id: documentId, need: 'viewer' },
      ),
    );
    if (ok) {
      for (const candidate of requested) {
        if (
          candidate.type === 'resource' &&
          candidate.resourceKind === 'document' &&
          tryDecodeId('document', candidate.resourceId) === documentId
        )
          offer(candidate);
      }
      const resource = requested.find(
        (candidate): candidate is Extract<RealtimeSubscription, { type: 'resource' }> =>
          candidate.type === 'resource' &&
          candidate.resourceKind === 'document' &&
          tryDecodeId('document', candidate.resourceId) === documentId,
      );
      if (resource)
        offer({ type: 'access', resourceKind: 'document', resourceId: resource.resourceId });
    }
  }
  if (principal?.isOperator && path.startsWith('/o/isoastra/')) {
    offer({ type: 'query', resourceKind: 'configured', queryKey: 'operator:configuration' });
    offer({ type: 'query', resourceKind: 'realtime', queryKey: 'operator:realtime' });
  }
  return [...allowed.values()];
}

export async function registerView(raw: unknown): Promise<RegisteredView> {
  const input = viewRegistrationSchema.parse(raw);
  const principal = await currentPrincipal();
  const subscriptions = await authorize(input.path, input.subscriptions);
  const lease = new Date(Date.now() + LEASE_MS);
  const personId = principal ? String(principal.personId) : null;
  const sessionId = principal?.sessionId ?? null;
  await database().transaction(async (tx) => {
    await tx
      .insert(schema.realtimeView)
      .values({
        id: input.viewId,
        visitorId: input.visitorId,
        tabId: input.tabId,
        sessionId,
        personId,
        path: input.path,
        title: input.title,
        visible: input.viewport.visible,
        visibleSections: input.viewport.sectionIds,
        visibleRows: input.viewport.rowIds,
        interactedAt: input.interactedAt ? new Date(input.interactedAt) : null,
        leaseExpiresAt: lease,
      })
      .onConflictDoUpdate({
        target: [schema.realtimeView.visitorId, schema.realtimeView.tabId],
        set: {
          id: input.viewId,
          sessionId,
          personId,
          path: input.path,
          title: input.title,
          visible: input.viewport.visible,
          visibleSections: input.viewport.sectionIds,
          visibleRows: input.viewport.rowIds,
          interactedAt: input.interactedAt ? new Date(input.interactedAt) : null,
          lastHeartbeatAt: new Date(),
          leaseExpiresAt: lease,
          disconnectedAt: null,
        },
      });
    await tx
      .delete(schema.realtimeSubscription)
      .where(eq(schema.realtimeSubscription.viewId, input.viewId));
    if (subscriptions.length)
      await tx
        .insert(schema.realtimeSubscription)
        .values(
          subscriptions.map((descriptor) => ({
            viewId: input.viewId,
            subscriptionKey: subscriptionKey(descriptor),
            descriptor,
          })),
        );
  });
  await pool().query(`SELECT pg_notify('realtime_presence',$1)`, [input.viewId]);
  return { viewId: input.viewId, subscriptions };
}

export async function disconnectView(visitorId: string, viewId: string): Promise<void> {
  await database()
    .update(schema.realtimeView)
    .set({ disconnectedAt: new Date(), leaseExpiresAt: new Date() })
    .where(
      and(eq(schema.realtimeView.id, viewId), eq(schema.realtimeView.visitorId, visitorId)),
    );
}

export async function pendingDeliveries(visitorId: string, viewId: string) {
  const principal = await currentPrincipal();
  const views = await database()
    .select()
    .from(schema.realtimeView)
    .where(
      and(eq(schema.realtimeView.id, viewId), eq(schema.realtimeView.visitorId, visitorId)),
    )
    .limit(1);
  const view = views[0];
  if (!view || view.personId !== (principal ? String(principal.personId) : null)) return [];
  const existing = await database()
    .select()
    .from(schema.realtimeSubscription)
    .where(eq(schema.realtimeSubscription.viewId, viewId));
  const subscriptions = await authorize(
    view.path,
    existing.map((row) => row.descriptor),
  );
  if (
    subscriptions.map(subscriptionKey).sort().join('|') !==
    existing
      .map((row) => row.subscriptionKey)
      .sort()
      .join('|')
  ) {
    await database().transaction(async (tx) => {
      await tx
        .delete(schema.realtimeSubscription)
        .where(eq(schema.realtimeSubscription.viewId, viewId));
      if (subscriptions.length)
        await tx
          .insert(schema.realtimeSubscription)
          .values(
            subscriptions.map((descriptor) => ({
              viewId,
              subscriptionKey: subscriptionKey(descriptor),
              descriptor,
            })),
          );
    });
  }
  return database()
    .select({
      id: schema.realtimeDelivery.id,
      eventSeq: schema.realtimeDelivery.eventSeq,
      revision: schema.realtimeDelivery.revision,
      reason: schema.realtimeDelivery.reason,
      resourceKind: schema.domainEvent.resourceKind,
      resourceId: schema.domainEvent.resourceId,
      kind: schema.domainEvent.kind,
      published: schema.domainEvent.published,
    })
    .from(schema.realtimeDelivery)
    .innerJoin(schema.realtimeView, eq(schema.realtimeView.id, schema.realtimeDelivery.viewId))
    .innerJoin(
      schema.domainEvent,
      and(
        eq(schema.domainEvent.orgId, schema.realtimeDelivery.eventOrgId),
        eq(schema.domainEvent.seq, schema.realtimeDelivery.eventSeq),
      ),
    )
    .where(
      and(
        eq(schema.realtimeDelivery.viewId, viewId),
        eq(schema.realtimeView.visitorId, visitorId),
        sql`${schema.realtimeDelivery.appliedAt} IS NULL`,
      ),
    )
    .orderBy(schema.realtimeDelivery.eventSeq)
    .limit(100);
}

export async function markSent(ids: string[]): Promise<void> {
  if (ids.length)
    await database()
      .update(schema.realtimeDelivery)
      .set({ sentAt: new Date() })
      .where(inArray(schema.realtimeDelivery.id, ids));
}

export async function inspectRealtime() {
  const views = await database()
    .select()
    .from(schema.realtimeView)
    .where(gt(schema.realtimeView.lastHeartbeatAt, sql`now() - interval '7 days'`))
    .orderBy(desc(schema.realtimeView.lastHeartbeatAt));
  const ids = views.map((view) => view.id);
  const subscriptions = ids.length
    ? await database()
        .select()
        .from(schema.realtimeSubscription)
        .where(inArray(schema.realtimeSubscription.viewId, ids))
    : [];
  const deliveries = ids.length
    ? await database()
        .select()
        .from(schema.realtimeDelivery)
        .where(inArray(schema.realtimeDelivery.viewId, ids))
        .orderBy(desc(schema.realtimeDelivery.selectedAt))
        .limit(500)
    : [];
  return { views, subscriptions, deliveries };
}

export function etag(body: unknown): string {
  return createHash('sha256').update(JSON.stringify(body)).digest('base64url').slice(0, 16);
}
export { randomUUID, type ViewRegistration };
