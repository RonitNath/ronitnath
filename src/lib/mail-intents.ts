import { createHmac, randomUUID } from 'node:crypto';
import { pool } from '@/db/client';
import { MailDeliveryError, sendMail, type Message } from '@/lib/mail';

export type MailIntentKind = 'verify_email' | 'reset_password';
export type MailAttemptResult =
  | { outcome: 'accepted'; provider: string; receiptId?: string }
  | { outcome: 'retry'; code: string; expiresAt: string; retryAfterMs?: number }
  | { outcome: 'permanent'; code: string }
  | { outcome: 'expired' | 'superseded' };

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
    WHERE state IN ('pending','dispatching') AND expires_at<=now()`);
  return result.rowCount ?? 0;
}

export async function markMailDispatched(id: string, generation: number, workflowId: string): Promise<void> {
  await pool().query(`UPDATE mail_intent SET state='dispatching',workflow_id=$2
    WHERE id=$1 AND dispatch_generation=$3 AND state='pending'`, [id, workflowId, generation]);
}

export async function deliverMailIntent(id: string, generation: number, ordinal: number): Promise<MailAttemptResult> {
  const result = await pool().query<{ message: Message | null; expires_at: Date; state: string; dispatch_generation: number }>(
    `SELECT message,expires_at,state,dispatch_generation FROM mail_intent WHERE id=$1`, [id]);
  const intent = result.rows[0];
  if (!intent || intent.dispatch_generation !== generation) return { outcome: 'superseded' };
  if (intent.state === 'accepted' || intent.state === 'failed' || intent.state === 'expired') return { outcome: 'superseded' };
  if (!intent.message || intent.expires_at.getTime() <= Date.now()) return { outcome: 'expired' };
  await pool().query(`INSERT INTO mail_attempt(intent_id,generation,ordinal,outcome,provider_invocations)
    VALUES($1,$2,$3,'started',1) ON CONFLICT(intent_id,generation,ordinal) DO UPDATE
    SET outcome='started',provider_invocations=mail_attempt.provider_invocations+1,attempted_at=now()`, [id, generation, ordinal]);
  try {
    const acceptance = await sendMail(intent.message, Math.min(10_000, intent.expires_at.getTime() - Date.now()));
    return { outcome: 'accepted', provider: acceptance.provider, ...(acceptance.receiptId ? { receiptId: acceptance.receiptId } : {}) };
  } catch (error) {
    const failure = error instanceof MailDeliveryError
      ? error : new MailDeliveryError(safeDetail(error), 'ambiguous', 'unknown');
    if (failure.kind === 'permanent') return { outcome: 'permanent', code: failure.code };
    return { outcome: 'retry', code: failure.code, expiresAt: intent.expires_at.toISOString(),
      ...(failure.retryAfterMs === undefined ? {} : { retryAfterMs: failure.retryAfterMs }) };
  }
}

export async function recordMailAttempt(
  id: string,
  generation: number,
  ordinal: number,
  result: MailAttemptResult,
  finalAttempt: boolean,
): Promise<'accepted' | 'retry' | 'failed' | 'expired' | 'superseded'> {
  const client = await pool().connect();
  try {
    await client.query('BEGIN');
    const locked = await client.query<{ dispatch_generation: number }>(
      `SELECT dispatch_generation FROM mail_intent WHERE id=$1 FOR UPDATE`, [id]);
    if (!locked.rows[0] || locked.rows[0].dispatch_generation !== generation || result.outcome === 'superseded') {
      await client.query('ROLLBACK');
      return 'superseded';
    }
    const terminalState = result.outcome === 'accepted' ? 'accepted'
      : result.outcome === 'expired' ? 'expired'
      : result.outcome === 'permanent' || finalAttempt ? 'failed' : null;
    const detail = result.outcome === 'accepted'
      ? JSON.stringify({ provider: result.provider, ...(result.receiptId ? { receiptId: result.receiptId } : {}) })
      : 'code' in result ? result.code : null;
    const evidenceOutcome = terminalState ?? result.outcome;
    await client.query(`INSERT INTO mail_attempt(intent_id,generation,ordinal,outcome,detail) VALUES($1,$2,$3,$4,$5)
      ON CONFLICT(intent_id,generation,ordinal) DO UPDATE SET outcome=EXCLUDED.outcome,detail=EXCLUDED.detail`,
      [id, generation, ordinal, evidenceOutcome, detail]);
    if (terminalState) await client.query(`UPDATE mail_intent SET state=$2,message=NULL,completed_at=now() WHERE id=$1`, [id, terminalState]);
    await client.query('COMMIT');
    return terminalState ?? 'retry';
  } catch (error) {
    await client.query('ROLLBACK');
    throw error;
  } finally {
    client.release();
  }
}
