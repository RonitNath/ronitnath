import { describe, expect, it } from 'vitest';

import { DEFAULT_MINUTES, calendarUid, renderCalendar } from '../ics';

const base = {
  uid: calendarUid('board-games', 'ronitnath.com'),
  sequence: 0,
  title: 'Board games and hanging out',
  startsAt: new Date('2026-08-30T21:00:00Z'),
  endsAt: new Date('2026-08-31T02:00:00Z'),
  location: "Ronit's apartment",
  description: 'No theme, no stakes.',
  url: 'https://ronitnath.com/e/board-games',
  organizer: null,
  stamp: new Date('2026-08-01T12:00:00Z'),
};

describe('renderCalendar', () => {
  it('renders one VEVENT with CRLF line endings', () => {
    const ics = renderCalendar(base);
    expect(ics.startsWith('BEGIN:VCALENDAR\r\n')).toBe(true);
    expect(ics.endsWith('END:VCALENDAR\r\n')).toBe(true);
    expect(ics.split('BEGIN:VEVENT').length - 1).toBe(1);
    expect(ics).toContain('UID:board-games@ronitnath.com');
    expect(ics).toContain('DTSTART:20260830T210000Z');
    expect(ics).toContain('DTEND:20260831T020000Z');
    expect(ics).toContain('DTSTAMP:20260801T120000Z');
    expect(ics).toContain('SEQUENCE:0');
  });

  it('is deterministic: nothing in it reads the clock', () => {
    expect(renderCalendar(base)).toBe(renderCalendar({ ...base }));
  });

  it('gives an event with no end a default length', () => {
    const ics = renderCalendar({ ...base, endsAt: null });
    const end = new Date(base.startsAt.getTime() + DEFAULT_MINUTES * 60_000);
    expect(ics).toContain(`DTEND:${end.toISOString().replace(/[-:]/g, '').slice(0, 15)}Z`);
  });

  it('escapes the characters RFC 5545 reserves', () => {
    const ics = renderCalendar({
      ...base,
      title: 'Games, snacks; bring a friend\nor two',
      description: 'A back\\slash',
    });
    expect(ics).toContain('SUMMARY:Games\\, snacks\\; bring a friend\\nor two');
    expect(ics).toContain('DESCRIPTION:A back\\\\slash');
  });

  it('folds long lines at 75 octets and continues them with a space', () => {
    const ics = renderCalendar({ ...base, title: 'x'.repeat(200) });
    for (const line of ics.split('\r\n')) {
      expect(Buffer.from(line, 'utf8').length).toBeLessThanOrEqual(75);
    }
    expect(ics).toMatch(/\r\n /);
    /* Unfolding gives the title back exactly. */
    const unfolded = ics.replace(/\r\n /g, '');
    expect(unfolded).toContain(`SUMMARY:${'x'.repeat(200)}`);
  });

  it('bumps SEQUENCE so a calendar replaces what it holds', () => {
    expect(renderCalendar({ ...base, sequence: 3 })).toContain('SEQUENCE:3');
  });

  it('cancels rather than disappears', () => {
    const ics = renderCalendar({ ...base, cancelled: true });
    expect(ics).toContain('METHOD:CANCEL');
    expect(ics).toContain('STATUS:CANCELLED');
  });
});
