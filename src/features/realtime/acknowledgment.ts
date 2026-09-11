import { and, eq } from 'drizzle-orm';
import { deliveryAcknowledgmentSchema } from '@isoastra/fleet-events/presence';
import { database, schema } from '@/db/client';

export async function acknowledgeView(
  visitorId: string,
  viewId: string,
  raw: unknown,
): Promise<boolean> {
  const ack = deliveryAcknowledgmentSchema.parse(raw);
  const owner = await database()
    .select({ id: schema.realtimeView.id })
    .from(schema.realtimeView)
    .where(
      and(eq(schema.realtimeView.id, viewId), eq(schema.realtimeView.visitorId, visitorId)),
    )
    .limit(1);
  if (!owner.length) return false;
  const values =
    ack.stage === 'received' ? { receivedAt: new Date() } : { appliedAt: new Date() };
  const rows = await database()
    .update(schema.realtimeDelivery)
    .set(values)
    .where(
      and(
        eq(schema.realtimeDelivery.id, ack.deliveryId),
        eq(schema.realtimeDelivery.viewId, viewId),
        eq(schema.realtimeDelivery.revision, ack.revision),
      ),
    )
    .returning({ id: schema.realtimeDelivery.id });
  return rows.length > 0;
}
