import { afterEach, describe, expect, it, vi } from 'vitest';
import { MailDeliveryError, sendMail, verificationMail } from '../mail';

const message = verificationMail('person@example.com', 'https://example.test/secret-token');

afterEach(() => {
  delete process.env.USESEND_API_URL;
  delete process.env.USESEND_API_KEY;
  vi.unstubAllGlobals();
});

describe('mail delivery classification', () => {
  it('accepts a successful provider response', async () => {
    process.env.USESEND_API_URL = 'https://mail.test';
    process.env.USESEND_API_KEY = 'test';
    vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 202 })));
    await expect(sendMail(message)).resolves.toBeUndefined();
  });

  it('classifies throttling and bounds Retry-After', async () => {
    process.env.USESEND_API_URL = 'https://mail.test';
    process.env.USESEND_API_KEY = 'test';
    vi.stubGlobal('fetch', vi.fn(async () => new Response('later', { status: 429, headers: { 'retry-after': '9999' } })));
    const error = await sendMail(message).catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(MailDeliveryError);
    expect(error).toMatchObject({ kind: 'transient', retryAfterMs: 300_000 });
  });

  it('treats ordinary 4xx as permanent and network loss as ambiguous', async () => {
    process.env.USESEND_API_URL = 'https://mail.test';
    process.env.USESEND_API_KEY = 'test';
    vi.stubGlobal('fetch', vi.fn(async () => new Response('bad recipient', { status: 400 })));
    await expect(sendMail(message)).rejects.toMatchObject({ kind: 'permanent' });
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('socket closed'); }));
    await expect(sendMail(message)).rejects.toMatchObject({ kind: 'ambiguous' });
  });
});
