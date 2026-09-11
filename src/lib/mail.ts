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
  constructor(message: string, readonly kind: MailFailureKind, readonly retryAfterMs?: number) {
    super(message);
  }
}

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

async function sendViaUseSend(base: string, key: string, message: Message): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${base.replace(/\/$/, '')}/api/v1/emails`, {
      method: 'POST',
      headers: { authorization: `Bearer ${key}`, 'content-type': 'application/json' },
      body: JSON.stringify({ from: from(), to: message.to, subject: message.subject, text: message.text, html: message.html }),
    });
  } catch (error) {
    throw new MailDeliveryError(error instanceof Error ? error.message : String(error), 'ambiguous');
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
    throw new MailDeliveryError(`usesend: ${res.status}`, kind, retryAfterMs);
  }
}

/* A recipient the transport must refuse. `MAIL_FAIL=1` fails everything;
 * anything else is a substring of the address that fails, so one Playwright
 * run can watch a send fail for one visitor and succeed for the rest. */
function refuses(to: string): boolean {
  const fail = process.env.MAIL_FAIL;
  if (!fail) return false;
  return fail === '1' || to.includes(fail);
}

export async function sendMail(message: Message): Promise<void> {
  if (refuses(message.to)) throw new MailDeliveryError('mail: refused by MAIL_FAIL', 'permanent');
  const apiUrl = process.env.USESEND_API_URL;
  const apiKey = process.env.USESEND_API_KEY;
  if (apiUrl && apiKey) {
    await sendViaUseSend(apiUrl, apiKey, message);
    return;
  }
  const url = process.env.SMTP_URL;
  if (url) {
    try {
      await transport(url).sendMail({ from: from(), to: message.to, subject: message.subject, text: message.text, html: message.html });
    } catch (error) {
      throw new MailDeliveryError(error instanceof Error ? error.message : String(error), 'ambiguous');
    }
    return;
  }

  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  const dir = mailDir();
  const path = join(dir, `${stamp}-${Math.random().toString(36).slice(2, 8)}.eml`);
  await mkdir(dir, { recursive: true });
  await writeFile(path, asEml(message), 'utf8');
  console.log(
    JSON.stringify({ level: 'info', event: 'mail.console', to: message.to, subject: message.subject, path }),
  );
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
