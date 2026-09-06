import { describe, expect, it } from 'vitest';

import {
  best,
  decide,
  expandSubjects,
  isLastOwner,
  isLevel,
  isRole,
  rankOf,
  wouldCycle,
  type Level,
  type MembershipEdge,
  type Role,
  type Subject,
} from '../authority';

const person = (id: number): Subject => ({ kind: 'person', id });
const group = (id: number): Subject => ({ kind: 'group', id });
const org = (id: number): Subject => ({ kind: 'organization', id });

function edge(subject: Subject, resource: Subject): MembershipEdge {
  return { subject, resource };
}

const names = (rows: Subject[]) => rows.map((row) => `${row.kind}:${row.id}`).sort();

describe('subject expansion', () => {
  it('is the person alone when they are in nothing', () => {
    expect(names(expandSubjects(person(1), []))).toEqual(['person:1']);
  });

  it('reaches the groups and organizations a person is in', () => {
    const edges = [edge(person(1), group(10)), edge(person(1), org(2)), edge(person(9), group(11))];
    expect(names(expandSubjects(person(1), edges))).toEqual([
      'group:10',
      'organization:2',
      'person:1',
    ]);
  });

  it('follows nesting: a group inside a group inside an organization', () => {
    const edges = [
      edge(person(1), group(10)),
      edge(group(10), group(11)),
      edge(group(11), org(2)),
    ];
    expect(names(expandSubjects(person(1), edges))).toEqual([
      'group:10',
      'group:11',
      'organization:2',
      'person:1',
    ]);
  });

  it('terminates on a cycle instead of walking it', () => {
    const edges = [
      edge(person(1), group(10)),
      edge(group(10), group(11)),
      edge(group(11), group(10)),
    ];
    expect(names(expandSubjects(person(1), edges))).toEqual(['group:10', 'group:11', 'person:1']);
  });

  it('does not reach the members of a group the person is in', () => {
    const edges = [edge(person(1), group(10)), edge(person(2), group(10))];
    expect(names(expandSubjects(person(1), edges))).toEqual(['group:10', 'person:1']);
  });
});

describe('cycle refusal', () => {
  const edges = [edge(group(11), group(10)), edge(group(12), group(11))];

  it('refuses a group inside itself', () => {
    expect(wouldCycle(edges, group(10), group(10))).toBe(true);
  });

  it('refuses a group inside one of its own members', () => {
    /* 11 is already in 10, so 10 may not go into 11 — nor into 12, which is
     * two hops below it. */
    expect(wouldCycle(edges, group(11), group(10))).toBe(true);
    expect(wouldCycle(edges, group(12), group(10))).toBe(true);
  });

  it('allows a group that is nowhere near', () => {
    expect(wouldCycle(edges, group(10), group(20))).toBe(false);
    expect(wouldCycle(edges, group(10), group(12))).toBe(false);
  });
});

describe('the two vocabularies', () => {
  it('nests roles member < admin < owner', () => {
    expect(rankOf('organization', 'member')).toBeLessThan(rankOf('organization', 'admin'));
    expect(rankOf('organization', 'admin')).toBeLessThan(rankOf('organization', 'owner'));
    expect(rankOf('group', 'owner')).toBe(rankOf('organization', 'owner'));
  });

  it('nests levels viewer < commenter < editor, under the owner', () => {
    expect(rankOf('document', 'viewer')).toBeLessThan(rankOf('document', 'commenter'));
    expect(rankOf('document', 'commenter')).toBeLessThan(rankOf('document', 'editor'));
    expect(rankOf('document', 'editor')).toBeLessThan(rankOf('document', 'owner'));
  });

  it('knows nothing about a verb from the other vocabulary', () => {
    expect(rankOf('document', 'admin')).toBe(0);
    expect(rankOf('organization', 'editor')).toBe(0);
    expect(isRole('admin')).toBe(true);
    expect(isRole('editor')).toBe(false);
    expect(isLevel('viewer')).toBe(true);
    expect(isLevel('owner')).toBe(false);
  });
});

describe('allows', () => {
  const me = [person(1), group(10), org(2)];
  const held = (verbs: string[], ownerParty: number | null = null) => ({
    subjects: me,
    ownerParty,
    verbs,
  });

  it('answers the whole role table', () => {
    const table: [string[], Role, boolean][] = [
      [[], 'member', false],
      [['member'], 'member', true],
      [['member'], 'admin', false],
      [['member'], 'owner', false],
      [['admin'], 'member', true],
      [['admin'], 'admin', true],
      [['admin'], 'owner', false],
      [['owner'], 'member', true],
      [['owner'], 'admin', true],
      [['owner'], 'owner', true],
    ];
    for (const [verbs, need, expected] of table) {
      expect(decide(held(verbs), { on: 'organization', id: 5, need }, false)).toBe(expected);
    }
  });

  it('answers the whole level table', () => {
    const table: [string[], Level, boolean][] = [
      [[], 'viewer', false],
      [['viewer'], 'viewer', true],
      [['viewer'], 'commenter', false],
      [['viewer'], 'editor', false],
      [['commenter'], 'viewer', true],
      [['commenter'], 'editor', false],
      [['editor'], 'viewer', true],
      [['editor'], 'commenter', true],
      [['editor'], 'editor', true],
    ];
    for (const [verbs, need, expected] of table) {
      expect(decide(held(verbs), { on: 'document', id: 5, need }, false)).toBe(expected);
    }
  });

  it('takes the highest of several verbs, from any of the actor’s subjects', () => {
    expect(decide(held(['viewer', 'editor']), { on: 'document', id: 5, need: 'editor' }, false))
      .toBe(true);
  });

  it('gives the owning party every rank, and only to a party', () => {
    /* The document belongs to the organization the actor is in. */
    expect(decide(held([], 2), { on: 'document', id: 5, need: 'editor' }, false)).toBe(true);
    expect(best(held([], 2), 'document')).toBe('owner');
    /* 10 is a group id, not a party id: it must not match an owning party. */
    expect(decide(held([], 10), { on: 'document', id: 5, need: 'viewer' }, false)).toBe(false);
    expect(decide(held([], 99), { on: 'document', id: 5, need: 'viewer' }, false)).toBe(false);
  });

  it('ignores a verb from the other vocabulary', () => {
    expect(decide(held(['admin']), { on: 'document', id: 5, need: 'viewer' }, false)).toBe(false);
    expect(decide(held(['editor']), { on: 'group', id: 5, need: 'member' }, false)).toBe(false);
    expect(best(held(['admin']), 'document')).toBeNull();
  });

  it('ends in the operator clause', () => {
    for (const on of ['organization', 'group', 'document'] as const) {
      expect(decide(held([]), { on, id: 5, need: 'owner' }, false)).toBe(false);
      expect(decide(held([]), { on, id: 5, need: 'owner' }, true)).toBe(true);
    }
  });
});

describe('the last owner', () => {
  it('cannot leave, and everybody else can', () => {
    expect(isLastOwner([1], 1)).toBe(true);
    expect(isLastOwner([1, 2], 1)).toBe(false);
    expect(isLastOwner([1, 2], 3)).toBe(false);
    expect(isLastOwner([], 1)).toBe(false);
  });
});
