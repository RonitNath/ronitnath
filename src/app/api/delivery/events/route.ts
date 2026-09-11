import { timingSafeEqual } from 'node:crypto';
import { RunEventSchema, type RunEvent } from '@isoastra/fleet-delivery';
import { sql } from 'drizzle-orm';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { emit } from '@/lib/fleet/events';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function authorized(request: Request): boolean {
  const expected = process.env.DELIVERY_INGEST_TOKEN;
  const actual = request.headers.get('authorization')?.replace(/^Bearer\s+/i, '');
  if (!expected || !actual) return false;
  const a = Buffer.from(actual); const b = Buffer.from(expected);
  return a.length === b.length && timingSafeEqual(a, b);
}

export async function POST(request: Request): Promise<Response> {
  if (!authorized(request)) return new Response(null, { status: 404 });
  const payload = await request.json();
  const events = z.array(RunEventSchema).min(1).max(1000).parse(Array.isArray(payload) ? payload : [payload]);
  const accepted = await database().transaction(async (tx) => {
    let inserted = 0;
    for (const event of events as RunEvent[]) {
      await tx.insert(schema.deliveryRun).values({ id: event.runId, requestedSha: event.type === 'run-requested' ? event.sha : 'unknown', state: 'running', lastSeq: event.seq, startedAt: new Date(event.at), updatedAt: new Date(event.at) }).onConflictDoUpdate({
        target: schema.deliveryRun.id,
        set: {
          requestedSha: event.type === 'run-requested' ? event.sha : sql`${schema.deliveryRun.requestedSha}`,
          state: event.type === 'run-finished' ? event.state : sql`${schema.deliveryRun.state}`,
          lastSeq: sql`greatest(${schema.deliveryRun.lastSeq}, ${event.seq})`,
          updatedAt: new Date(event.at),
          finishedAt: event.type === 'run-finished' ? new Date(event.at) : sql`${schema.deliveryRun.finishedAt}`,
        },
      });
      const rows = await tx.insert(schema.deliveryRunEvent).values({ runId: event.runId, seq: event.seq, event, at: new Date(event.at) }).onConflictDoNothing().returning({ seq: schema.deliveryRunEvent.seq });
      inserted += rows.length;
    }
    await tx.delete(schema.deliveryRunEvent).where(sql`${schema.deliveryRunEvent.at} < now() - interval '30 days'`);
    await tx.delete(schema.deliveryRun).where(sql`${schema.deliveryRun.startedAt} < now() - interval '1 year'`);
    await emit(tx, { orgId: 'isoastra', resourceKind: 'delivery-run', resourceId: events[0]!.runId, kind: 'delivery.telemetry', payload: { accepted: inserted } });
    return inserted;
  });
  return Response.json({ accepted });
}
