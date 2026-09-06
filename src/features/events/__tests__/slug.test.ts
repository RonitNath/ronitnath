import { describe, expect, it } from 'vitest';

import { slugCandidate, slugify } from '../slug';

describe('slugify', () => {
  it('makes a URL word out of a title', () => {
    expect(slugify('Board games and hanging out')).toBe('board-games-and-hanging-out');
  });

  it('folds accents rather than percent-encoding them', () => {
    expect(slugify('Café soirée')).toBe('cafe-soiree');
  });

  it('never returns an empty slug', () => {
    expect(slugify('!!! ???')).toBe('event');
    expect(slugify('   ')).toBe('event');
  });

  it('drops punctuation and collapses runs', () => {
    expect(slugify('Sam & Priya’s (2026) party!!')).toBe('sam-priya-s-2026-party');
  });

  it('bounds the length and never ends on a hyphen', () => {
    const slug = slugify('a'.repeat(20) + ' ' + 'b'.repeat(60));
    expect(slug.length).toBeLessThanOrEqual(48);
    expect(slug.endsWith('-')).toBe(false);
  });
});

describe('slugCandidate', () => {
  it('offers the pretty one first', () => {
    expect(slugCandidate('Board games', 0)).toBe('board-games');
  });

  it('suffixes the next few, and stays inside the bound', () => {
    expect(slugCandidate('Board games', 1)).toBe('board-games-2');
    expect(slugCandidate('Board games', 2)).toBe('board-games-3');
    const long = slugCandidate('x'.repeat(60), 3);
    expect(long.length).toBeLessThanOrEqual(48);
  });

  it('stops counting once counting stops being informative', () => {
    const candidate = slugCandidate('Board games', 12);
    expect(candidate).toMatch(/^board-games-[a-z0-9]{4}$/);
  });
});
