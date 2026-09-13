import { timingSafeEqual } from 'node:crypto';
import { and, eq, isNotNull, lt, sql } from 'drizzle-orm';
import { z } from 'zod';

import { authSchema, database, schema } from '@/db/client';
import { OPERATOR_RESOURCE } from '@/features/auth/principal';
import { emit } from '@/lib/fleet/events';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const bodySchema = z.discriminatedUnion('action', [
  z.object({ action: z.literal('start'), releaseId: z.string().uuid(), fixtureId: z.string().uuid() }).strict(),
  z.object({ action: z.literal('finish'), fixtureId: z.string().uuid(), viewId: z.string().uuid(), eventSeq: z.number().int().nonnegative() }).strict(),
  z.object({ action: z.literal('cleanup'), fixtureId: z.string().uuid() }).strict(),
]);

function authorized(request: Request): boolean {
  const expected = process.env.DELIVERY_ACCEPTANCE_TOKEN;
  const actual = request.headers.get('authorization')?.replace(/^Bearer\s+/i, '');
  if (!expected || !actual) return false;
  const a = Buffer.from(actual);
  const b = Buffer.from(expected);
  return a.length === b.length && timingSafeEqual(a, b);
}

async function existingOperator(): Promise<boolean> {
  const email = process.env.DELIVERY_ACCEPTANCE_EMAIL;
  if (!email) return false;
  const rows = await database()
    .select({ id: schema.person.id })
    .from(authSchema.user)
    .innerJoin(schema.person, eq(schema.person.userId, authSchema.user.id))
    .innerJoin(
      schema.relation,
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.subjectId, schema.person.id),
        eq(schema.relation.verb, 'operator'),
        eq(schema.relation.resourceKind, OPERATOR_RESOURCE.kind),
        eq(schema.relation.resourceId, OPERATOR_RESOURCE.id),
      ),
    )
    .where(eq(authSchema.user.email, email))
    .limit(1);
  return rows.length === 1;
}

export async function POST(request: Request): Promise<Response> {
  if (!authorized(request)) return new Response(null, { status: 404 });
  const parsed = bodySchema.safeParse(await request.json().catch(() => null));
  if (!parsed.success) return Response.json({ error: 'invalid acceptance command' }, { status: 400 });
  const input = parsed.data;
  await database()
    .delete(schema.deliveryAcceptanceFixture)
    .where(lt(schema.deliveryAcceptanceFixture.expiresAt, new Date()));
  if (input.action === 'cleanup') {
    await database()
      .delete(schema.deliveryAcceptanceFixture)
      .where(eq(schema.deliveryAcceptanceFixture.id, input.fixtureId));
    return new Response(null, { status: 204 });
  }
  if (input.action === 'start') {
    if (!(await existingOperator()))
      return Response.json({ error: 'acceptance operator is absent or revoked' }, { status: 409 });
    const eventSeq = await database().transaction(async (tx) => {
      await tx.insert(schema.deliveryAcceptanceFixture).values({
        id: input.fixtureId,
        releaseId: input.releaseId,
        expiresAt: new Date(Date.now() + 10 * 60_000),
      });
      return emit(tx, {
        orgId: 'isoastra',
        resourceKind: 'configured',
        resourceId: 'homepage',
        kind: 'delivery.acceptance',
        payload: { fixtureId: input.fixtureId, releaseId: input.releaseId },
      });
    });
    return Response.json({ eventSeq, operator: true });
  }
  const rows = await database()
    .select({ id: schema.realtimeDelivery.id })
    .from(schema.deliveryAcceptanceFixture)
    .innerJoin(
      schema.realtimeDelivery,
      and(
        eq(schema.realtimeDelivery.viewId, input.viewId),
        eq(schema.realtimeDelivery.eventOrgId, 'isoastra'),
        eq(schema.realtimeDelivery.eventSeq, input.eventSeq),
        isNotNull(schema.realtimeDelivery.receivedAt),
        isNotNull(schema.realtimeDelivery.appliedAt),
      ),
    )
    .innerJoin(
      schema.domainEvent,
      and(
        eq(schema.domainEvent.orgId, schema.realtimeDelivery.eventOrgId),
        eq(schema.domainEvent.seq, schema.realtimeDelivery.eventSeq),
      ),
    )
    .where(
      and(
        eq(schema.deliveryAcceptanceFixture.id, input.fixtureId),
        sql`${schema.domainEvent.payload}->>'fixtureId' = ${input.fixtureId}`,
      ),
    )
    .limit(1);
  if (!rows.length)
    return Response.json({ error: 'receipt/application acknowledgment is incomplete' }, { status: 409 });
  await database()
    .delete(schema.deliveryAcceptanceFixture)
    .where(eq(schema.deliveryAcceptanceFixture.id, input.fixtureId));
  return Response.json({ operator: true, received: true, applied: true });
}
