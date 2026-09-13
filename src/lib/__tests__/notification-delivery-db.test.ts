import { randomUUID } from 'node:crypto';
import { afterAll, describe, expect, it } from 'vitest';
import { closePool, pool } from '@/db/client';
import { notificationStore, persistAuthDeliveryIntent } from '@/lib/notification-delivery';

const reachable = Boolean(process.env.TEST_DATABASE_URL);
const created: string[] = [];
afterAll(async () => {
  if (created.length) await pool().query(`DELETE FROM fleet_delivery_intent WHERE tenant_id='ronitnath' AND id=ANY($1::text[])`, [created]);
  await closePool();
});

describe.skipIf(!reachable)('shared authentication delivery evidence', () => {
  it('persists intent and outbox atomically, deduplicates, and purges content on acceptance', async () => {
    process.env.MAIL_INTENT_KEY = 'notification-delivery-test-key';
    const message = { to: `${randomUUID()}@example.test`, subject: 'Test', text: 'secret body', html: '<p>secret body</p>' };
    const expires = new Date(Date.now() + 60_000);
    const first = await persistAuthDeliveryIntent('verify_email', message, expires, 'correlation-1');
    created.push(first);
    const second = await persistAuthDeliveryIntent('verify_email', message, expires, 'correlation-2');
    expect(second).toBe(first);
    const persisted = await pool().query(`SELECT kind,address,recipient_id,correlation_id,message FROM fleet_delivery_intent
      WHERE tenant_id='ronitnath' AND id=$1`, [first]);
    expect(persisted.rows[0]).toMatchObject({ kind: 'verify_email', address: message.to, correlation_id: 'correlation-1' });
    expect(persisted.rows[0].recipient_id).not.toContain(message.to);
    const outbox = await pool().query(`SELECT state FROM fleet_notification_outbox WHERE tenant_id='ronitnath' AND id=$1`, [`delivery:${first}`]);
    expect(outbox.rows[0]).toMatchObject({ state: 'pending' });
    const intent = await notificationStore().deliveryIntent('ronitnath', first, 1);
    expect(intent).not.toBeNull();
    await notificationStore().beginDeliveryAttempt(intent!, 1);
    await notificationStore().recordDeliveryAttempt(intent!, 1, { outcome: 'accepted', provider: 'test', receiptId: 'receipt' }, false);
    const terminal = await pool().query(`SELECT state,message FROM fleet_delivery_intent WHERE tenant_id='ronitnath' AND id=$1`, [first]);
    expect(terminal.rows[0]).toMatchObject({ state: 'accepted', message: null });
    const attempt = await pool().query(`SELECT outcome,provider,receipt_id,code FROM fleet_delivery_attempt
      WHERE tenant_id='ronitnath' AND intent_id=$1`, [first]);
    expect(attempt.rows[0]).toMatchObject({ outcome: 'accepted', provider: 'test', receipt_id: 'receipt', code: null });
  });
});
