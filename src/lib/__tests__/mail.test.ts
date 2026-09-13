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
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ id: 'receipt-1' }, { status: 202 })));
    await expect(sendMail(message)).resolves.toEqual({ provider: 'usesend', receiptId: 'receipt-1' });
  });

  it('classifies throttling and bounds Retry-After', async () => {
    process.env.USESEND_API_URL = 'https://mail.test';
    process.env.USESEND_API_KEY = 'test';
    vi.stubGlobal('fetch', vi.fn(async () => new Response('later', { status: 429, headers: { 'retry-after': '9999' } })));
    const error = await sendMail(message).catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(MailDeliveryError);
    expect(error).toMatchObject({ kind: 'transient', code: 'http_429', retryAfterMs: 300_000 });
  });

  it('treats ordinary 4xx as permanent and network loss as ambiguous', async () => {
    process.env.USESEND_API_URL = 'https://mail.test';
    process.env.USESEND_API_KEY = 'test';
    vi.stubGlobal('fetch', vi.fn(async () => new Response('bad recipient', { status: 400 })));
    await expect(sendMail(message)).rejects.toMatchObject({ kind: 'permanent', code: 'http_400' });
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('socket closed'); }));
    await expect(sendMail(message)).rejects.toMatchObject({ kind: 'ambiguous', code: 'network_error' });
  });

  it('bounds a provider call and classifies timeout as ambiguous', async () => {
    process.env.USESEND_API_URL = 'https://mail.test';
    process.env.USESEND_API_KEY = 'test';
    vi.stubGlobal('fetch', vi.fn((_url: string, init: RequestInit) => new Promise((_resolve, reject) => {
      init.signal?.addEventListener('abort', () => reject(init.signal?.reason), { once: true });
    })));
    await expect(sendMail(message, 5)).rejects.toMatchObject({ kind: 'ambiguous', code: 'network_timeout' });
  });
});
