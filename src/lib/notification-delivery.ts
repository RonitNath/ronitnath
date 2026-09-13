import { createHmac, randomUUID } from 'node:crypto';
import { PostgresNotificationStore } from '@isoastra/fleet-notifications/postgres';
import { pool } from '@/db/client';
import { emailTransportRegistration, type Message } from '@/lib/mail';

export type AuthMailKind = 'verify_email' | 'reset_password';
export const AUTH_NOTIFICATION_TENANT = 'ronitnath';
export const AUTH_MAIL_CHANNEL = 'auth-email';

function secret(): string {
  const value = process.env.MAIL_INTENT_KEY ?? process.env.ID_KEY;
  if (!value) throw new Error('MAIL_INTENT_KEY or ID_KEY is required');
  return value;
}
function digest(parts: readonly unknown[]): string { return createHmac('sha256', secret()).update(JSON.stringify(parts)).digest('hex'); }

export const authNotificationAuthorization = {
  canRead: async () => false,
  canDeliver: async (input: { tenantId: string; channel: string }) => input.tenantId === AUTH_NOTIFICATION_TENANT && input.channel === AUTH_MAIL_CHANNEL,
};
export function notificationStore(): PostgresNotificationStore { return new PostgresNotificationStore(pool(), authNotificationAuthorization); }

export async function persistAuthDeliveryIntent(kind: AuthMailKind, message: Message, expiresAt: Date, correlationId: string): Promise<string> {
  const registration = await emailTransportRegistration();
  const retention = registration.transport.deduplication.retentionMs;
  const client = await pool().connect();
  try {
    await client.query('BEGIN');
    const id = await notificationStore().createDelivery(client, {
      tenantId: AUTH_NOTIFICATION_TENANT,
      id: randomUUID(),
      kind,
      channel: AUTH_MAIL_CHANNEL,
      recipientId: digest(['recipient', message.to]),
      address: message.to,
      correlationId,
      message,
      idempotencyKey: digest([kind, message.to, message.subject, message.text, message.html]),
      expiresAt: expiresAt.toISOString(),
      ambiguousPolicy: 'retry',
      providerDeduplication: registration.transport.deduplication.guaranteed,
      ...(retention ? { deduplicationExpiresAt: new Date(Date.now() + retention).toISOString() } : {}),
    });
    await client.query('COMMIT');
    return id;
  } catch (error) {
    await client.query('ROLLBACK');
    throw error;
  } finally { client.release(); }
}
