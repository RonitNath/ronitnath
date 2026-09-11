import { createHmac, randomUUID } from 'node:crypto';
import { pool } from '@/db/client';
import { MailDeliveryError, sendMail, type Message } from '@/lib/mail';

export type MailIntentKind = 'verify_email' | 'reset_password';
export type MailAttemptResult =
  | { outcome: 'accepted' | 'permanent' | 'expired' }
  | { outcome: 'retry'; retryAfterMs?: number };

function secret(): string {
  const value = process.env.MAIL_INTENT_KEY ?? process.env.ID_KEY;
  if (!value) throw new Error('MAIL_INTENT_KEY or ID_KEY is required');
  return value;
}

function deduplicationKey(kind: MailIntentKind, message: Message): string {
  return createHmac('sha256', secret())
    .update(JSON.stringify([kind, message.to, message.subject, message.text, message.html]))
    .digest('hex');
}

export async function persistMailIntent(
  kind: MailIntentKind,
  message: Message,
  expiresAt: Date,
  correlationId: string,
): Promise<string> {
  const id = randomUUID();
  const result = await pool().query<{ id: string }>(`
    INSERT INTO mail_intent(id,deduplication_key,kind,recipient,message,correlation_id,expires_at)
    VALUES($1,$2,$3,$4,$5::jsonb,$6,$7)
    ON CONFLICT(deduplication_key) DO UPDATE SET deduplication_key=EXCLUDED.deduplication_key
    RETURNING id`, [id, deduplicationKey(kind, message), kind, message.to, JSON.stringify(message), correlationId, expiresAt]);
  return result.rows[0]!.id;
}

function safeDetail(error: unknown): string {
  return (error instanceof Error ? error.message : String(error)).replace(/https?:\/\/\S+/g, '[url]').slice(0, 200);
}

export async function pendingMailIntents(limit = 100): Promise<{ id: string; generation: number }[]> {
  const result = await pool().query<{ id: string; dispatch_generation: number }>(`
    SELECT id,dispatch_generation FROM mail_intent WHERE state='pending' AND expires_at>now()
    ORDER BY created_at LIMIT $1`, [limit]);
  return result.rows.map((row) => ({ id: row.id, generation: row.dispatch_generation }));
}

export async function expirePendingMailIntents(): Promise<number> {
  const result = await pool().query(`UPDATE mail_intent SET state='expired',message=NULL,completed_at=now()
    WHERE state='pending' AND expires_at<=now()`);
  return result.rowCount ?? 0;
}

export async function markMailDispatched(id: string, workflowId: string): Promise<void> {
  await pool().query(`UPDATE mail_intent SET state='dispatching',workflow_id=$2
    WHERE id=$1 AND state='pending'`, [id, workflowId]);
}

export async function attemptMailIntent(id: string, ordinal: number, finalAttempt: boolean): Promise<MailAttemptResult> {
  const result = await pool().query<{ message: Message | null; expires_at: Date; state: string }>(
    `SELECT message,expires_at,state FROM mail_intent WHERE id=$1`, [id]);
  const intent = result.rows[0];
  if (!intent || intent.state === 'accepted' || intent.state === 'failed' || intent.state === 'expired') {
    return { outcome: 'permanent' };
  }
  if (!intent.message || intent.expires_at.getTime() <= Date.now()) {
    await complete(id, ordinal, 'expired', null);
    return { outcome: 'expired' };
  }
  try {
    await sendMail(intent.message);
    await complete(id, ordinal, 'accepted', null);
    return { outcome: 'accepted' };
  } catch (error) {
    const failure = error instanceof MailDeliveryError ? error : new MailDeliveryError(safeDetail(error), 'ambiguous');
    const terminal = failure.kind === 'permanent' || finalAttempt;
    if (terminal) {
      await complete(id, ordinal, 'failed', safeDetail(failure));
      return { outcome: 'permanent' };
    }
    await pool().query(`INSERT INTO mail_attempt(intent_id,ordinal,outcome,detail) VALUES($1,$2,$3,$4)
      ON CONFLICT(intent_id,ordinal) DO NOTHING`, [id, ordinal, failure.kind, safeDetail(failure)]);
    return { outcome: 'retry', ...(failure.retryAfterMs === undefined ? {} : { retryAfterMs: failure.retryAfterMs }) };
  }
}

async function complete(id: string, ordinal: number, outcome: 'accepted' | 'expired' | 'failed', detail: string | null): Promise<void> {
  const client = await pool().connect();
  try {
    await client.query('BEGIN');
    await client.query(`INSERT INTO mail_attempt(intent_id,ordinal,outcome,detail) VALUES($1,$2,$3,$4)
      ON CONFLICT(intent_id,ordinal) DO NOTHING`, [id, ordinal, outcome, detail]);
    await client.query(`UPDATE mail_intent SET state=$2,message=NULL,completed_at=now() WHERE id=$1`, [id, outcome]);
    await client.query('COMMIT');
  } catch (error) {
    await client.query('ROLLBACK');
    throw error;
  } finally {
    client.release();
  }
}
