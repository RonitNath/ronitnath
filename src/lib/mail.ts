/* Outbound mail. One transport, chosen by the environment.
 *
 * `USESEND_API_URL` + `USESEND_API_KEY`: the fleet mailer's REST API
 * (POST /api/v1/emails, verified against usesend's email-schema.ts). This is
 * the production path — the useSend SMTP proxy on the mesh advertises
 * STARTTLS without a certificate, so no SMTP client can authenticate to it.
 * `SMTP_URL`: nodemailer, for any plain SMTP relay. Without either — every dev machine and the
 * Playwright run — the message is written to `.mail/<timestamp>.eml`, because
 * a test that has to click a verification link needs to read one. Token-bearing
 * message bodies are never copied into process logs.
 * The directory is gitignored; nothing else in the app reads it. */

import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import nodemailer, { type Transporter } from 'nodemailer';

export interface Message {
  to: string;
  subject: string;
  text: string;
  html: string;
}

export type MailFailureKind = 'transient' | 'permanent' | 'ambiguous';
export class MailDeliveryError extends Error {
  constructor(
    message: string,
    readonly kind: MailFailureKind,
    readonly code: string,
    readonly retryAfterMs?: number,
  ) {
    super(message);
  }
}
export interface MailAcceptance { provider: 'usesend' | 'smtp' | 'file'; receiptId?: string }

/* Where the dev transport writes. `MAIL_DIR` exists because Next's standalone
 * server chdirs into its own directory, so "the repo root" is not something
 * the process can work out for itself — the Playwright run points it back. */
export function mailDir(): string {
  return process.env.MAIL_DIR ?? join(process.cwd(), '.mail');
}

function from(): string {
  return process.env.MAIL_FROM ?? 'Ronit Nath <no-reply@ronitnath.com>';
}

const globalForMail = globalThis as unknown as { rnMailer?: Transporter };

function transport(url: string): Transporter {
  globalForMail.rnMailer ??= nodemailer.createTransport(url);
  return globalForMail.rnMailer;
}

/* An .eml a mail client would open, and a Playwright test can grep. */
function asEml(message: Message): string {
  return [
    `From: ${from()}`,
    `To: ${message.to}`,
    `Subject: ${message.subject}`,
    `Date: ${new Date().toUTCString()}`,
    'MIME-Version: 1.0',
    'Content-Type: text/plain; charset=utf-8',
    '',
    message.text,
    '',
  ].join('\r\n');
}

async function sendViaUseSend(base: string, key: string, message: Message, timeoutMs: number): Promise<MailAcceptance> {
  let res: Response;
  try {
    res = await fetch(`${base.replace(/\/$/, '')}/api/v1/emails`, {
      method: 'POST',
      headers: { authorization: `Bearer ${key}`, 'content-type': 'application/json' },
      body: JSON.stringify({ from: from(), to: message.to, subject: message.subject, text: message.text, html: message.html }),
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (error) {
    const timeout = error instanceof Error && (error.name === 'TimeoutError' || error.name === 'AbortError');
    throw new MailDeliveryError(timeout ? 'mail provider timed out' : 'mail provider connection failed', 'ambiguous', timeout ? 'network_timeout' : 'network_error');
  }
  if (!res.ok) {
    const raw = res.headers.get('retry-after');
    const seconds = raw && /^\d+$/.test(raw) ? Number(raw) : null;
    const dateDelay = raw && seconds === null ? Date.parse(raw) - Date.now() : NaN;
    const requestedDelay = seconds === null ? dateDelay : seconds * 1_000;
    const retryAfterMs = Number.isFinite(requestedDelay)
      ? Math.max(0, Math.min(300_000, requestedDelay))
      : undefined;
    const kind: MailFailureKind = res.status === 429 || res.status >= 500 ? 'transient' : 'permanent';
    await res.body?.cancel().catch(() => undefined);
    throw new MailDeliveryError(`usesend: ${res.status}`, kind, `http_${res.status}`, retryAfterMs);
  }
  let receiptId: string | undefined;
  try {
    const body = await res.json() as Record<string, unknown>;
    if (typeof body.id === 'string') receiptId = body.id.slice(0, 200);
  } catch { await res.body?.cancel().catch(() => undefined); }
  return { provider: 'usesend', ...(receiptId ? { receiptId } : {}) };
}

/* A recipient the transport must refuse. `MAIL_FAIL=1` fails everything;
 * anything else is a substring of the address that fails, so one Playwright
 * run can watch a send fail for one visitor and succeed for the rest. */
function refuses(to: string): boolean {
  const fail = process.env.MAIL_FAIL;
  if (!fail) return false;
  return fail === '1' || to.includes(fail);
}

export async function sendMail(message: Message, timeoutMs = 10_000): Promise<MailAcceptance> {
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new MailDeliveryError('mail intent expired', 'permanent', 'expired');
  if (refuses(message.to)) throw new MailDeliveryError('mail: refused by MAIL_FAIL', 'permanent', 'test_refused');
  const apiUrl = process.env.USESEND_API_URL;
  const apiKey = process.env.USESEND_API_KEY;
  if (apiUrl && apiKey) {
    return sendViaUseSend(apiUrl, apiKey, message, timeoutMs);
  }
  const url = process.env.SMTP_URL;
  if (url) {
    try {
      const active = transport(url);
      let timer: ReturnType<typeof setTimeout> | undefined;
      const timeout = new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          active.close();
          globalForMail.rnMailer = undefined;
          reject(new MailDeliveryError('SMTP provider timed out', 'ambiguous', 'network_timeout'));
        }, timeoutMs);
      });
      const info = await Promise.race([
        active.sendMail({ from: from(), to: message.to, subject: message.subject, text: message.text, html: message.html }),
        timeout,
      ]).finally(() => { if (timer) clearTimeout(timer); });
      return { provider: 'smtp', ...(typeof info.messageId === 'string' ? { receiptId: info.messageId.slice(0, 200) } : {}) };
    } catch (error) {
      if (error instanceof MailDeliveryError) throw error;
      const responseCode = typeof error === 'object' && error && 'responseCode' in error
        ? Number((error as { responseCode?: unknown }).responseCode) : 0;
      const kind: MailFailureKind = responseCode >= 500 ? 'permanent' : responseCode >= 400 ? 'transient' : 'ambiguous';
      throw new MailDeliveryError('SMTP provider rejected or lost the request', kind, responseCode ? `smtp_${responseCode}` : 'smtp_unknown');
    }
  }

  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  const dir = mailDir();
  const path = join(dir, `${stamp}-${Math.random().toString(36).slice(2, 8)}.eml`);
  await mkdir(dir, { recursive: true });
  await writeFile(path, asEml(message), 'utf8');
  console.log(
    JSON.stringify({ level: 'info', event: 'mail.console', to: message.to, subject: message.subject, path }),
  );
  return { provider: 'file' };
}

export async function closeMailTransport(): Promise<void> {
  const current = globalForMail.rnMailer;
  globalForMail.rnMailer = undefined;
  if (current) current.close();
}

/* The letters this rung sends. Both are one sentence, one link and the
 * time it dies: an email that explains itself at length reads as a phish. */

export function verificationMail(to: string, url: string): Message {
  const text = `Confirm this address to finish setting up your account on ronitnath.com.\n\n${url}\n\nThe link works once and expires in 1 hour. If you did not register, ignore this message.\n`;
  return {
    to,
    subject: 'Confirm your email',
    text,
    html: `<p>Confirm this address to finish setting up your account on ronitnath.com.</p><p><a href="${url}">${url}</a></p><p>The link works once and expires in 1 hour. If you did not register, ignore this message.</p>`,
  };
}

export function passwordResetMail(to: string, url: string): Message {
  const text = `Set a new password for your account on ronitnath.com.\n\n${url}\n\nThe link works once and expires in 1 hour. If you did not ask for it, ignore this message; your password has not changed.\n`;
  return {
    to,
    subject: 'Reset your password',
    text,
    html: `<p>Set a new password for your account on ronitnath.com.</p><p><a href="${url}">${url}</a></p><p>The link works once and expires in 1 hour. If you did not ask for it, ignore this message; your password has not changed.</p>`,
  };
}
