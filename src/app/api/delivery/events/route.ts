import { timingSafeEqual } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import {
  EnvironmentEventV3Schema,
  StoredRunEventSchema,
  type EnvironmentEventV3,
  type StoredRunEvent,
} from '@isoastra/fleet-delivery';
import { and, eq, sql } from 'drizzle-orm';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { emit } from '@/lib/fleet/events';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function authorized(request: Request): boolean {
  const expected = process.env.DELIVERY_INGEST_TOKEN;
  const actual = request.headers.get('authorization')?.replace(/^Bearer\s+/i, '');
  if (!expected || !actual) return false;
  const a = Buffer.from(actual);
  const b = Buffer.from(expected);
  return a.length === b.length && timingSafeEqual(a, b);
}
type DeliveryEvent = StoredRunEvent | EnvironmentEventV3;
const DeliveryEventSchema = z.union([StoredRunEventSchema, EnvironmentEventV3Schema]);
function identity(event: DeliveryEvent) {
  if (event.schemaVersion === 3)
    return { runId: event.instanceId, producerId: event.producerId };
  return event.schemaVersion === 2
    ? { runId: event.releaseId, producerId: event.producerId }
    : { runId: event.runId, producerId: 'legacy' };
}
function requestedSha(event: DeliveryEvent): string | undefined {
  if (event.schemaVersion === 3) return `environment:${event.instance.mode}`;
  if (event.schemaVersion === 2 && event.type === 'release-requested')
    return event.requestedSha;
  if (
    event.schemaVersion === 1 &&
    event.type === 'run-requested' &&
    typeof event.sha === 'string'
  )
    return event.sha;
}
function terminalState(event: DeliveryEvent): string | undefined {
  if (event.schemaVersion === 3) return event.instance.state;
  if (event.schemaVersion === 2 && event.type === 'release-finished') return event.state;
  if (
    event.schemaVersion === 2 &&
    event.type === 'coordinator-checkpoint' &&
    event.state !== 'deploying'
  )
    return event.state;
  if (
    event.schemaVersion === 1 &&
    event.type === 'run-finished' &&
    typeof event.state === 'string'
  )
    return event.state;
}

export async function POST(request: Request): Promise<Response> {
  if (!authorized(request)) return new Response(null, { status: 404 });
  const payload: unknown = await request.json();
  const events = z
    .array(DeliveryEventSchema)
    .min(1)
    .max(1000)
    .parse(Array.isArray(payload) ? payload : [payload]);
  try {
    const accepted = await database().transaction(async (tx) => {
      let inserted = 0;
      for (const event of events) {
        const { runId, producerId } = identity(event);
        await tx
          .insert(schema.deliveryRun)
          .values({
            id: runId,
            requestedSha: requestedSha(event) ?? 'unknown',
            state: 'running',
            lastSeq: -1,
            startedAt: new Date(event.at),
            updatedAt: new Date(event.at),
          })
          .onConflictDoNothing();
        const rows = await tx
          .insert(schema.deliveryRunEvent)
          .values({ runId, producerId, seq: event.seq, event, at: new Date(event.at) })
          .onConflictDoNothing()
          .returning({ seq: schema.deliveryRunEvent.seq });
        if (!rows.length) {
          const [existing] = await tx
            .select({ event: schema.deliveryRunEvent.event })
            .from(schema.deliveryRunEvent)
            .where(
              and(
                eq(schema.deliveryRunEvent.runId, runId),
                eq(schema.deliveryRunEvent.producerId, producerId),
                eq(schema.deliveryRunEvent.seq, event.seq),
              ),
            )
            .limit(1);
          if (!isDeepStrictEqual(existing?.event, event))
            throw new Error('conflicting delivery event identity');
          continue;
        }
        inserted++;
        const state = terminalState(event);
        const at = new Date(event.at);
        const sha = requestedSha(event);
        await tx
          .update(schema.deliveryRun)
          .set({
            requestedSha: sha
              ? sql`case when ${schema.deliveryRun.requestedSha} = 'unknown' then ${sha} else ${schema.deliveryRun.requestedSha} end`
              : sql`${schema.deliveryRun.requestedSha}`,
            state: state
              ? sql`case when ${schema.deliveryRun.updatedAt} <= ${at} then ${state} else ${schema.deliveryRun.state} end`
              : sql`${schema.deliveryRun.state}`,
            lastSeq: sql`greatest(${schema.deliveryRun.lastSeq}, ${event.seq})`,
            updatedAt: sql`greatest(${schema.deliveryRun.updatedAt}, ${at})`,
            finishedAt: state
              ? sql`greatest(coalesce(${schema.deliveryRun.finishedAt}, ${at}), ${at})`
              : sql`${schema.deliveryRun.finishedAt}`,
          })
          .where(eq(schema.deliveryRun.id, runId));
      }
      await tx
        .delete(schema.deliveryRunEvent)
        .where(sql`${schema.deliveryRunEvent.at} < now() - interval '30 days'`);
      await tx
        .delete(schema.deliveryRun)
        .where(sql`${schema.deliveryRun.startedAt} < now() - interval '1 year'`);
      if (inserted)
        await emit(tx, {
          orgId: 'isoastra',
          resourceKind: 'delivery-run',
          resourceId: identity(events[0]!).runId,
          kind: 'delivery.telemetry',
          payload: { accepted: inserted },
        });
      return inserted;
    });
    return Response.json({ accepted });
  } catch (error) {
    if (error instanceof Error && error.message === 'conflicting delivery event identity')
      return Response.json({ error: error.message }, { status: 409 });
    throw error;
  }
}
