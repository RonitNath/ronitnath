import { createHash } from 'node:crypto';
import { join } from 'node:path';
import {
  createFileEmailTransport,
  createSmtpEmailTransport,
  createUseSendEmailTransport,
  type EmailResult,
  type EmailTransport,
} from '@isoastra/fleet-notifications/email';

export interface Message { to: string; subject: string; text: string; html: string }
export type MailFailureKind = 'transient' | 'permanent' | 'ambiguous';
export class MailDeliveryError extends Error {
  constructor(message: string, readonly kind: MailFailureKind, readonly code: string, readonly retryAfterMs?: number) { super(message); }
}
export interface MailAcceptance { provider: 'usesend' | 'smtp' | 'file'; receiptId?: string }

export function mailDir(): string { return process.env.MAIL_DIR ?? join(process.cwd(), '.mail'); }
function sender(): string { return process.env.MAIL_FROM ?? 'Ronit Nath <no-reply@ronitnath.com>'; }

const globalForMail = globalThis as unknown as { rnSharedMailer?: { key: string; transport: EmailTransport } };

function configuredTransport(): EmailTransport {
  const apiUrl = process.env.USESEND_API_URL;
  const apiKey = process.env.USESEND_API_KEY;
  const smtpUrl = process.env.SMTP_URL;
  const key = apiUrl && apiKey ? `usesend:${apiUrl}` : smtpUrl ? `smtp:${smtpUrl}` : `file:${mailDir()}`;
  if (globalForMail.rnSharedMailer?.key === key) return globalForMail.rnSharedMailer.transport;
  void globalForMail.rnSharedMailer?.transport.close?.();
  let transport: EmailTransport;
  if (apiUrl && apiKey) {
    const retention = Number(process.env.USESEND_IDEMPOTENCY_RETENTION_MS);
    transport = createUseSendEmailTransport({
      baseUrl: apiUrl,
      apiKey,
      deduplication: Number.isFinite(retention) && retention > 0 ? { guaranteed: true, retentionMs: retention } : { guaranteed: false },
      fetch: (input, init) => fetch(input, init),
    });
  } else if (smtpUrl) {
    transport = createSmtpEmailTransport({ url: smtpUrl, deduplication: { guaranteed: false } });
  } else {
    transport = createFileEmailTransport({ directory: mailDir() });
  }
  globalForMail.rnSharedMailer = { key, transport };
  return transport;
}

export async function emailTransportRegistration(): Promise<{ transport: EmailTransport; from: string }> {
  const base = configuredTransport();
  const transport: EmailTransport = {
    provider: base.provider,
    deduplication: base.deduplication,
    send: async (request) => refuses(request.to)
      ? { outcome: 'rejected', code: 'test_refused' }
      : base.send(request),
  };
  return { transport, from: sender() };
}

function refuses(to: string): boolean {
  const fail = process.env.MAIL_FAIL;
  return Boolean(fail && (fail === '1' || to.includes(fail)));
}

function asFailure(result: Exclude<EmailResult, { outcome: 'accepted' }>): MailDeliveryError {
  if (result.outcome === 'rejected') return new MailDeliveryError('mail provider rejected the message', 'permanent', result.code);
  return new MailDeliveryError('mail provider did not confirm acceptance', result.outcome === 'retry' ? 'transient' : 'ambiguous', result.code, result.retryAfterMs);
}

/* Compatibility entrypoint for workflows created before the notification migration. */
export async function sendMail(message: Message, timeoutMs = 10_000): Promise<MailAcceptance> {
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new MailDeliveryError('mail intent expired', 'permanent', 'expired');
  if (refuses(message.to)) throw new MailDeliveryError('mail: refused by MAIL_FAIL', 'permanent', 'test_refused');
  const registration = await emailTransportRegistration();
  const result = await registration.transport.send({
    to: message.to,
    from: registration.from,
    message,
    idempotencyKey: createHash('sha256').update(JSON.stringify(message)).digest('hex'),
    timeoutMs,
  });
  if (result.outcome !== 'accepted') throw asFailure(result);
  return { provider: result.provider as MailAcceptance['provider'], ...(result.receiptId ? { receiptId: result.receiptId } : {}) };
}

export async function closeMailTransport(): Promise<void> {
  const current = globalForMail.rnSharedMailer;
  globalForMail.rnSharedMailer = undefined;
  await current?.transport.close?.();
}

export function verificationMail(to: string, url: string): Message {
  const text = `Confirm this address to finish setting up your account on ronitnath.com.\n\n${url}\n\nThe link works once and expires in 1 hour. If you did not register, ignore this message.\n`;
  return { to, subject: 'Confirm your email', text, html: `<p>Confirm this address to finish setting up your account on ronitnath.com.</p><p><a href="${url}">${url}</a></p><p>The link works once and expires in 1 hour. If you did not register, ignore this message.</p>` };
}

export function passwordResetMail(to: string, url: string): Message {
  const text = `Set a new password for your account on ronitnath.com.\n\n${url}\n\nThe link works once and expires in 1 hour. If you did not ask for it, ignore this message; your password has not changed.\n`;
  return { to, subject: 'Reset your password', text, html: `<p>Set a new password for your account on ronitnath.com.</p><p><a href="${url}">${url}</a></p><p>The link works once and expires in 1 hour. If you did not ask for it, ignore this message; your password has not changed.</p>` };
}
