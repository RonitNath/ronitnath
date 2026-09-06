import { describe, expect, it } from 'vitest';

import { firstName, orderGuests, type Guest } from '../ordering';

function guest(id: number, name: string, minutes: number): Guest {
  return {
    personId: id,
    displayName: name,
    answeredAt: new Date(Date.UTC(2026, 7, 30, 12, minutes)),
    plusOne: 0,
  };
}

const guests = [guest(1, 'Sam Okafor', 30), guest(2, 'Priya Rao', 10), guest(3, 'Alex Yun', 20)];

describe('orderGuests', () => {
  it('falls back to answered-order when nobody shares a circle', () => {
    expect(orderGuests(guests, new Set()).map((row) => row.personId)).toEqual([2, 3, 1]);
  });

  it('puts people the viewer shares a circle with first', () => {
    const ordered = orderGuests(guests, new Set([1]));
    expect(ordered.map((row) => row.personId)).toEqual([1, 2, 3]);
    expect(ordered[0]!.shared).toBe(true);
    expect(ordered[1]!.shared).toBe(false);
  });

  it('keeps answered-order inside each half', () => {
    expect(orderGuests(guests, new Set([1, 3])).map((row) => row.personId)).toEqual([3, 1, 2]);
  });

  it('is stable for two answers in the same instant', () => {
    const tie = [guest(9, 'Nine', 5), guest(4, 'Four', 5)];
    expect(orderGuests(tie, new Set()).map((row) => row.personId)).toEqual([4, 9]);
  });
});

describe('firstName', () => {
  it('is what a guest reads before answering', () => {
    expect(firstName('Sam Okafor')).toBe('Sam');
    expect(firstName('  Priya  Rao ')).toBe('Priya');
    expect(firstName('Cher')).toBe('Cher');
  });
});
