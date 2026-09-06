/* Outbound mail. One transport, chosen by the environment.
 *
 * `USESEND_API_URL` + `USESEND_API_KEY`: the fleet mailer's REST API
 * (POST /api/v1/emails, verified against usesend's email-schema.ts). This is
 * the production path — the useSend SMTP proxy on the mesh advertises
 * STARTTLS without a certificate, so no SMTP client can authenticate to it.
 * `SMTP_URL`: nodemailer, for any plain SMTP relay. Without either — every dev machine and the
 * Playwright run — the message is logged to stdout *and* written to
 * `.mail/<timestamp>.eml`, because a test that has to click a verification
 * link needs to read one, and scraping a log is worse than opening a file.
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
  const res = await fetch(`${base.replace(/\/$/, '')}/api/v1/emails`, {
    method: 'POST',
    headers: { authorization: `Bearer ${key}`, 'content-type': 'application/json' },
    body: JSON.stringify({
      from: from(),
      to: message.to,
      subject: message.subject,
      text: message.text,
      html: message.html,
    }),
  });
  if (!res.ok) {
    throw new Error(`usesend: ${res.status} ${(await res.text()).slice(0, 200)}`);
  }
}

export async function sendMail(message: Message): Promise<void> {
  const apiUrl = process.env.USESEND_API_URL;
  const apiKey = process.env.USESEND_API_KEY;
  if (apiUrl && apiKey) {
    await sendViaUseSend(apiUrl, apiKey, message);
    return;
  }
  const url = process.env.SMTP_URL;
  if (url) {
    await transport(url).sendMail({
      from: from(),
      to: message.to,
      subject: message.subject,
      text: message.text,
      html: message.html,
    });
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
  console.log(message.text);
}

/* The two letters this rung sends. Both are one sentence, one link and the
 * time it dies: an email that explains itself at length reads as a phish. */

export function verificationMail(to: string, url: string): Message {
  const text = `Confirm this address to finish setting up your account on ronitnath.com.\n\n${url}\n\nThe link works once and expires in 24 hours. If you did not register, ignore this message.\n`;
  return {
    to,
    subject: 'Confirm your email',
    text,
    html: `<p>Confirm this address to finish setting up your account on ronitnath.com.</p><p><a href="${url}">${url}</a></p><p>The link works once and expires in 24 hours. If you did not register, ignore this message.</p>`,
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
