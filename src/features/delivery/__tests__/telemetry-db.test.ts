import { randomUUID } from 'node:crypto';
import { and, eq } from 'drizzle-orm';
import { afterAll, describe, expect, it } from 'vitest';

import { POST } from '@/app/api/delivery/events/route';
import { POST as accept } from '@/app/api/delivery/acceptance/route';
import { authSchema, schema } from '@/db/client';
import { OPERATOR_RESOURCE } from '@/features/auth/principal';
import { closeDatabase, database, reachable } from '@/lib/fleet/__tests__/db';

afterAll(closeDatabase);

const token = 'delivery-test-token';
process.env.DELIVERY_INGEST_TOKEN = token;
process.env.DELIVERY_ACCEPTANCE_TOKEN = 'delivery-acceptance-test-token';
process.env.DELIVERY_ACCEPTANCE_EMAIL = 'delivery-seed@example.com';

async function ingest(event: unknown) {
  return POST(
    new Request('http://localhost/api/delivery/events', {
      method: 'POST',
      headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
      body: JSON.stringify(event),
    }),
  );
}

describe.skipIf(!reachable)('delivery telemetry projection', () => {
  it('accepts idempotent duplicates, rejects conflicts, and cannot regress out of order', async () => {
    const releaseId = randomUUID();
    const producerId = 'runner';
    const earlier = new Date(Date.now() - 10_000).toISOString();
    const later = new Date().toISOString();
    const finished = {
      schemaVersion: 2,
      releaseId,
      producerId,
      seq: 2,
      at: later,
      type: 'release-finished',
      state: 'deployed',
      acceptance: true,
    };
    expect(await (await ingest(finished)).json()).toEqual({ accepted: 1 });
    expect(await (await ingest(finished)).json()).toEqual({ accepted: 0 });
    const requested = {
      schemaVersion: 2,
      releaseId,
      producerId,
      seq: 0,
      at: earlier,
      type: 'release-requested',
      requestedSha: 'a'.repeat(40),
    };
    expect(await (await ingest(requested)).json()).toEqual({ accepted: 1 });
    const [run] = await database()
      .select()
      .from(schema.deliveryRun)
      .where(eq(schema.deliveryRun.id, releaseId));
    expect(run).toMatchObject({ requestedSha: 'a'.repeat(40), state: 'deployed', lastSeq: 2 });
    expect(run!.updatedAt.toISOString()).toBe(later);
    const conflict = await ingest({ ...finished, state: 'failed' });
    expect(conflict.status).toBe(409);
    await database().delete(schema.deliveryRun).where(eq(schema.deliveryRun.id, releaseId));
  });
  it('uses an existing operator grant and never bootstraps a revoked identity', async () => {
    const fixtureId = randomUUID();
    const releaseId = randomUUID();
    const request = (action: unknown) =>
      accept(
        new Request('http://localhost/api/delivery/acceptance', {
          method: 'POST',
          headers: {
            authorization: 'Bearer delivery-acceptance-test-token',
            'content-type': 'application/json',
          },
          body: JSON.stringify(action),
        }),
      );
    const started = await request({ action: 'start', releaseId, fixtureId });
    expect(started.status).toBe(200);
    expect(await started.json()).toMatchObject({ operator: true });
    const cleaned = await request({ action: 'cleanup', fixtureId });
    expect(cleaned.status).toBe(204);
    const [operator] = await database()
      .select({ personId: schema.person.id })
      .from(authSchema.user)
      .innerJoin(schema.person, eq(schema.person.userId, authSchema.user.id))
      .where(eq(authSchema.user.email, 'delivery-seed@example.com'))
      .limit(1);
    expect(operator).toBeDefined();
    const grant = and(
      eq(schema.relation.subjectKind, 'person'),
      eq(schema.relation.subjectId, operator!.personId),
      eq(schema.relation.verb, 'operator'),
      eq(schema.relation.resourceKind, OPERATOR_RESOURCE.kind),
      eq(schema.relation.resourceId, OPERATOR_RESOURCE.id),
    );
    await database().delete(schema.relation).where(grant);
    const revoked = await request({ action: 'start', releaseId, fixtureId: randomUUID() });
    expect(revoked.status).toBe(409);
    expect(await database().select().from(schema.relation).where(grant)).toHaveLength(0);
    await database().insert(schema.relation).values({
      subjectKind: 'person',
      subjectId: operator!.personId,
      verb: 'operator',
      resourceKind: OPERATOR_RESOURCE.kind,
      resourceId: OPERATOR_RESOURCE.id,
    });
  });
});
