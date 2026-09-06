import { describe, expect, it } from 'vitest';

import { normalizeHandle } from '../handles';

describe('normalizeHandle', () => {
  it('reads an address as an address, folded the way a door is', () => {
    expect(normalizeHandle('  Bob@Example.COM ')).toEqual({
      kind: 'email',
      subject: 'bob@example.com',
      raw: 'Bob@Example.COM',
    });
  });

  it('keeps the local part case-sensitive nowhere and the domain nowhere else', () => {
    /* The same folding as auth's, so a handle and a confirmed address can be
     * compared at all: this is the whole of ProposeMatch. */
    expect(normalizeHandle('A.B+tag@Example.com')?.subject).toBe('a.b+tag@example.com');
  });

  it('reduces a phone number to its digits and keeps a country plus', () => {
    expect(normalizeHandle('+1 (415) 555-0123')).toMatchObject({
      kind: 'phone',
      subject: '+14155550123',
    });
    expect(normalizeHandle('415-555-0123')).toMatchObject({
      kind: 'phone',
      subject: '4155550123',
    });
  });

  it('infers no country code that was not typed', () => {
    expect(normalizeHandle('4155550123')?.subject).toBe('4155550123');
    expect(normalizeHandle('+4155550123')?.subject).toBe('+4155550123');
  });

  it('does not mistake a year or a house number for a phone number', () => {
    expect(normalizeHandle('2026')?.kind).toBe('name');
    expect(normalizeHandle('221b')?.kind).toBe('name');
    /* Longer than E.164 allows. */
    expect(normalizeHandle('1234567890123456')?.kind).toBe('name');
  });

  it('folds a bare name to lower case and single spaces, keeping what was typed', () => {
    expect(normalizeHandle('  Ravi   Menon ')).toEqual({
      kind: 'name',
      subject: 'ravi menon',
      raw: 'Ravi Menon',
    });
  });

  it('refuses nothing at all and refuses more than an address could be', () => {
    expect(normalizeHandle('   ')).toBeNull();
    expect(normalizeHandle('x'.repeat(255))).toBeNull();
  });
});
