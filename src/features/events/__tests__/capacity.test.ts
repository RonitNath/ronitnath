import { describe, expect, it } from 'vitest';

import { fullness, headcount, overflow } from '../capacity';

const rows = [
  { response: 'yes' as const, plusOne: 1 },
  { response: 'yes' as const, plusOne: 0 },
  { response: 'maybe' as const, plusOne: 2 },
  { response: 'no' as const, plusOne: 0 },
];

describe('headcount', () => {
  it('counts answers, and bodies only for a yes', () => {
    expect(headcount(rows)).toEqual({ yes: 2, maybe: 1, no: 1, coming: 3 });
  });

  it('is zero for nobody', () => {
    expect(headcount([])).toEqual({ yes: 0, maybe: 0, no: 0, coming: 0 });
  });
});

describe('fullness', () => {
  it('is never full without a capacity', () => {
    expect(fullness(null, rows)).toBe('open');
  });

  it('is full at the number, not past it', () => {
    expect(fullness(4, rows)).toBe('open');
    expect(fullness(3, rows)).toBe('full');
  });

  it('counts the answer that has not landed yet', () => {
    expect(fullness(4, rows, { response: 'yes', plusOne: 0 })).toBe('full');
    /* A maybe is not a seat. */
    expect(fullness(4, rows, { response: 'maybe', plusOne: 3 })).toBe('open');
  });
});

describe('overflow', () => {
  it('says how many bodies are past the line, never negative', () => {
    expect(overflow(2, rows)).toBe(1);
    expect(overflow(3, rows)).toBe(0);
    expect(overflow(null, rows)).toBe(0);
  });
});
