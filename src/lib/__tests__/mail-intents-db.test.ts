import { randomUUID } from 'node:crypto';
import { afterAll, describe, expect, it } from 'vitest';
import { closePool, pool } from '@/db/client';
import { persistMailIntent, recordMailAttempt } from '@/lib/mail-intents';

const reachable = Boolean(process.env.TEST_DATABASE_URL);
const created: string[] = [];
afterAll(async () => {
  if (created.length) await pool().query('DELETE FROM mail_intent WHERE id=ANY($1::uuid[])', [created]);
  await closePool();
});

describe.skipIf(!reachable)('durable mail evidence', () => {
  it('deduplicates immutable intents and purges after checkpointed acceptance', async () => {
    process.env.MAIL_INTENT_KEY = 'mail-intent-test-key';
    const message = { to: `${randomUUID()}@example.test`, subject: 'Test', text: 'body', html: '<p>body</p>' };
    const expires = new Date(Date.now() + 60_000);
    const first = await persistMailIntent('verify_email', message, expires, randomUUID());
    created.push(first);
    const second = await persistMailIntent('verify_email', message, expires, randomUUID());
    expect(second).toBe(first);
    await expect(recordMailAttempt(first, 0, 1,
      { outcome: 'accepted', provider: 'usesend', receiptId: 'receipt' }, false)).resolves.toBe('accepted');
    const intent = await pool().query('SELECT state,message FROM mail_intent WHERE id=$1', [first]);
    const attempts = await pool().query('SELECT generation,ordinal,outcome,detail FROM mail_attempt WHERE intent_id=$1', [first]);
    expect(intent.rows[0]).toMatchObject({ state: 'accepted', message: null });
    expect(attempts.rows[0]).toMatchObject({ generation: 0, ordinal: 1, outcome: 'accepted' });
    expect(attempts.rows[0].detail).not.toContain('body');
  });

  it('keeps generations independent and ignores stale workflows', async () => {
    const id = randomUUID();
    created.push(id);
    await pool().query(`INSERT INTO mail_intent(id,deduplication_key,kind,recipient,message,correlation_id,
      dispatch_generation,expires_at) VALUES($1,$2,'verify_email','test@example.test','{}','test',1,now()+interval '1 minute')`, [id, randomUUID()]);
    await expect(recordMailAttempt(id, 0, 1, { outcome: 'permanent', code: 'http_400' }, false)).resolves.toBe('superseded');
    await expect(recordMailAttempt(id, 1, 1,
      { outcome: 'retry', code: 'http_503', expiresAt: new Date(Date.now() + 60_000).toISOString() }, false)).resolves.toBe('retry');
    const intent = await pool().query('SELECT state,message IS NULL AS purged FROM mail_intent WHERE id=$1', [id]);
    expect(intent.rows[0]).toMatchObject({ state: 'pending', purged: false });
  });
});
