import { describe, expect, it } from 'vitest';

import { isAllowlisted, looksLikeEmail, normalizeEmail, oidcAllowlist } from '../email-address';

describe('normalizeEmail', () => {
  it('folds case and trims, and does nothing else', () => {
    expect(normalizeEmail('  Ronit@Isoastra.COM ')).toBe('ronit@isoastra.com');
    /* Dot-folding is a Gmail policy, not a mail one: two addresses that differ
     * by a dot are two people until a provider says otherwise. */
    expect(normalizeEmail('r.o.nit@gmail.com')).toBe('r.o.nit@gmail.com');
  });
});

describe('looksLikeEmail', () => {
  it('accepts an address with a dotted domain', () => {
    expect(looksLikeEmail('ronit@isoastra.com')).toBe(true);
    expect(looksLikeEmail('a+b@sub.example.co.uk')).toBe(true);
  });

  it('refuses what cannot be delivered', () => {
    for (const bad of ['', 'ronit', 'ronit@', '@isoastra.com', 'a b@c.com', 'a@b']) {
      expect(looksLikeEmail(bad)).toBe(false);
    }
    expect(looksLikeEmail(`${'a'.repeat(250)}@b.com`)).toBe(false);
  });
});

describe('the OIDC allowlist', () => {
  it('parses a comma- or space-separated list, normalised', () => {
    expect(oidcAllowlist('Ronit@Isoastra.com, other@example.com')).toEqual([
      'ronit@isoastra.com',
      'other@example.com',
    ]);
    expect(oidcAllowlist('')).toEqual([]);
    expect(oidcAllowlist(undefined)).toEqual([]);
  });

  it('answers membership case-insensitively', () => {
    expect(isAllowlisted('RONIT@isoastra.com', 'ronit@isoastra.com')).toBe(true);
    expect(isAllowlisted('someone@example.com', 'ronit@isoastra.com')).toBe(false);
  });

  it('is empty when nothing is configured, so no address gets the OIDC door', () => {
    expect(isAllowlisted('ronit@isoastra.com', '')).toBe(false);
  });
});
