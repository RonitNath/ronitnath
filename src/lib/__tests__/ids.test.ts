import { beforeEach, describe, expect, it } from 'vitest';
import { PublicIdError, decodeId, encodeId, tryDecodeId } from '../ids';

const KEY = '00112233445566778899aabbccddeeff';

beforeEach(() => {
  process.env.ID_KEY = KEY;
});

describe('public ids', () => {
  it('round trips every type', () => {
    for (const type of ['person', 'identity', 'organization', 'event'] as const) {
      for (const id of [1, 2, 42, 999_999, Number.MAX_SAFE_INTEGER]) {
        expect(decodeId(type, encodeId(type, id))).toBe(id);
      }
    }
  });

  it('is deterministic and does not leak the integer', () => {
    const a = encodeId('person', 7);
    expect(encodeId('person', 7)).toBe(a);
    expect(a).toMatch(/^p_[A-Za-z0-9_-]{22}$/);
    expect(a).not.toContain('7');
    expect(encodeId('person', 8)).not.toBe(a);
  });

  it('gives different types different ids for the same integer', () => {
    expect(encodeId('person', 7)).not.toBe(encodeId('event', 7).replace(/^e_/, 'p_'));
  });

  it('refuses a prefix mismatch', () => {
    const personId = encodeId('person', 7);
    expect(() => decodeId('event', personId)).toThrow(PublicIdError);
    expect(() => decodeId('event', personId.replace(/^p_/, 'e_'))).toThrow(PublicIdError);
    expect(tryDecodeId('event', personId)).toBeNull();
  });

  it('refuses a tampered body', () => {
    const id = encodeId('person', 7);
    const body = id.slice(2);
    const flipped = (body[0] === 'A' ? 'B' : 'A') + body.slice(1);
    expect(() => decodeId('person', `p_${flipped}`)).toThrow(PublicIdError);
    expect(() => decodeId('person', 'p_short')).toThrow(PublicIdError);
    expect(() => decodeId('person', 'p_' + '!'.repeat(22))).toThrow(PublicIdError);
  });

  it('refuses a key that is not 32 hex characters', () => {
    process.env.ID_KEY = 'nope';
    expect(() => encodeId('person', 1)).toThrow(PublicIdError);
  });

  it('refuses a value that is not an internal id', () => {
    expect(() => encodeId('person', 0)).toThrow(PublicIdError);
    expect(() => encodeId('person', -1)).toThrow(PublicIdError);
    expect(() => encodeId('person', 1.5)).toThrow(PublicIdError);
  });
});
