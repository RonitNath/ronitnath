import { describe, expect, it } from 'vitest';

import { planMerge, type IdentityRow, type RelationRow } from '../merge';

/* The properties the merge exists to hold, stated where they can be checked
 * without a database. The transaction that executes the plan is covered by
 * the Playwright golden flows. */

const SURVIVOR = 10;
const ABSORBED = 20;

function identity(id: number, personId: number, source: IdentityRow['source'], subject: string) {
  return { id, personId, source, subject };
}

function edge(
  id: number,
  subjectId: number,
  verb: string,
  resourceKind: string,
  resourceId: number,
): RelationRow {
  return { id, subjectKind: 'person', subjectId, verb, resourceKind, resourceId };
}

describe('planMerge', () => {
  it('moves what the survivor does not have and drops what it does', () => {
    const plan = planMerge({
      survivor: SURVIVOR,
      absorbed: ABSORBED,
      identities: [
        identity(1, SURVIVOR, 'local', 'bob@example.com'),
        identity(2, ABSORBED, 'handle', 'bob@example.com'),
        identity(3, ABSORBED, 'handle', '+14155550123'),
      ],
      relations: [],
    });
    /* The contact card's address and the confirmed address are one address. */
    expect(plan.dropIdentities).toEqual([2]);
    /* The phone number is how somebody knows them, and nothing else holds it. */
    expect(plan.moveIdentities).toEqual([3]);
  });

  it('never leaves the survivor holding the same subject twice', () => {
    const plan = planMerge({
      survivor: SURVIVOR,
      absorbed: ABSORBED,
      identities: [
        identity(1, SURVIVOR, 'local', 'bob@example.com'),
        identity(2, ABSORBED, 'handle', 'bob@example.com'),
        identity(3, ABSORBED, 'handle', 'bob@example.com'),
      ],
      relations: [],
    });
    expect(plan.moveIdentities).toEqual([]);
    expect(plan.dropIdentities).toEqual([2, 3]);
  });

  it('accounts for every identity the absorbed person held', () => {
    const identities = [
      identity(1, SURVIVOR, 'local', 'bob@example.com'),
      identity(2, ABSORBED, 'handle', 'bob@example.com'),
      identity(3, ABSORBED, 'handle', 'robert'),
      identity(4, ABSORBED, 'oidc', 'sub-1'),
    ];
    const plan = planMerge({ survivor: SURVIVOR, absorbed: ABSORBED, identities, relations: [] });
    const seen = [...plan.moveIdentities, ...plan.dropIdentities].sort();
    expect(seen).toEqual([2, 3, 4]);
  });

  it('loses no edge: every absorbed edge is copied or already on the survivor', () => {
    const relations: RelationRow[] = [
      /* Two members hold the same contact card. */
      edge(1, 30, 'contact', 'person', ABSORBED),
      edge(2, 31, 'contact', 'person', ABSORBED),
      /* The contact card was invited to something. */
      edge(3, ABSORBED, 'invited', 'event', 5),
      /* …which the survivor was invited to as well. */
      edge(4, SURVIVOR, 'invited', 'event', 5),
    ];
    const plan = planMerge({
      survivor: SURVIVOR,
      absorbed: ABSORBED,
      identities: [],
      relations,
    });
    const accounted = new Set([
      ...plan.copyEdges.map((row) => row.from),
      ...plan.skippedEdges.map((row) => row.from),
    ]);
    expect([...accounted].sort()).toEqual([1, 2, 3]);
    expect(plan.copyEdges.map((row) => row.edge.resourceId)).toEqual([SURVIVOR, SURVIVOR]);
    expect(plan.skippedEdges).toEqual([{ from: 3, reason: 'duplicate' }]);
  });

  it('writes no edge twice when two absorbed edges rewrite to the same one', () => {
    const plan = planMerge({
      survivor: SURVIVOR,
      absorbed: ABSORBED,
      identities: [],
      relations: [
        edge(1, 30, 'contact', 'person', ABSORBED),
        edge(2, 30, 'contact', 'person', SURVIVOR),
      ],
    });
    expect(plan.copyEdges).toEqual([]);
    expect(plan.skippedEdges).toEqual([{ from: 1, reason: 'duplicate' }]);
  });

  it('does not leave the claimant holding themselves', () => {
    /* The member who wrote the contact card turns out to be the person on it. */
    const plan = planMerge({
      survivor: SURVIVOR,
      absorbed: ABSORBED,
      identities: [],
      relations: [edge(1, SURVIVOR, 'contact', 'person', ABSORBED)],
    });
    expect(plan.copyEdges).toEqual([]);
    expect(plan.skippedEdges).toEqual([{ from: 1, reason: 'self' }]);
  });

  it('carries an edge the absorbed person held over anything else', () => {
    const plan = planMerge({
      survivor: SURVIVOR,
      absorbed: ABSORBED,
      identities: [],
      relations: [edge(1, ABSORBED, 'editor', 'document', 7)],
    });
    expect(plan.copyEdges).toEqual([
      {
        from: 1,
        edge: {
          subjectKind: 'person',
          subjectId: SURVIVOR,
          verb: 'editor',
          resourceKind: 'document',
          resourceId: 7,
        },
      },
    ]);
  });
});
