/* Key rotation, which is the only part of the id codec that is not already
 * covered by the codec's own suite. The property that matters is asymmetric:
 * an id minted under yesterday's key must still decode today, and an id minted
 * today must not depend on yesterday's key existing — otherwise the rotation
 * never finishes and the old key can never be destroyed. */

import { afterEach, describe, expect, it } from 'vitest';

import { PublicIdError, decodeId, encodeId } from '@/lib/fleet/ids';

const OLD = '00112233445566778899aabbccddeeff';
const NEW = 'ffeeddccbbaa99887766554433221100';

const original = { key: process.env.ID_KEY, prev: process.env.ID_KEY_PREV };

function keys(current: string, previous?: string) {
  process.env.ID_KEY = current;
  if (previous === undefined) delete process.env.ID_KEY_PREV;
  else process.env.ID_KEY_PREV = previous;
}

afterEach(() => {
  if (original.key === undefined) delete process.env.ID_KEY;
  else process.env.ID_KEY = original.key;
  if (original.prev === undefined) delete process.env.ID_KEY_PREV;
  else process.env.ID_KEY_PREV = original.prev;
});

describe('public ids under rotation', () => {
  it('decodes an id minted under the previous key', () => {
    keys(OLD);
    const minted = encodeId('person', 4172);

    keys(NEW, OLD);
    expect(decodeId('person', minted)).toBe(4172);
  });

  it('mints under the current key, and that id needs no previous key', () => {
    keys(NEW, OLD);
    const minted = encodeId('event', 9);

    keys(NEW);
    expect(decodeId('event', minted)).toBe(9);
  });

  it('mints the same id the current key alone would have minted', () => {
    keys(NEW);
    const alone = encodeId('organization', 77);
    keys(NEW, OLD);
    expect(encodeId('organization', 77)).toBe(alone);
  });

  it('refuses an old id once the previous key is gone', () => {
    keys(OLD);
    const minted = encodeId('person', 4172);

    keys(NEW);
    expect(() => decodeId('person', minted)).toThrow(PublicIdError);
  });

  it('still refuses the wrong type under either key', () => {
    keys(OLD);
    const person = encodeId('person', 4172);

    keys(NEW, OLD);
    expect(() => decodeId('document', person)).toThrow(PublicIdError);
  });
});
