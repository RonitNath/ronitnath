import { describe, expect, it } from 'vitest';
import { parseRange } from '../media';

describe('voice memo byte ranges', () => {
  it('supports bounded, open, and suffix ranges', () => {
    expect(parseRange('bytes=2-4', 10)).toEqual({ start: 2, end: 4 });
    expect(parseRange('bytes=7-', 10)).toEqual({ start: 7, end: 9 });
    expect(parseRange('bytes=-3', 10)).toEqual({ start: 7, end: 9 });
  });

  it('rejects multiple, empty, and out-of-bounds ranges', () => {
    expect(parseRange('bytes=0-1,4-5', 10)).toBe('invalid');
    expect(parseRange('bytes=-', 10)).toBe('invalid');
    expect(parseRange('bytes=10-', 10)).toBe('invalid');
  });
});
