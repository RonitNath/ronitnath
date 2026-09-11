/* Split, against a real database, because "the merge can be taken back" is a
 * property of rows in five tables and of the record the merge wrote about
 * itself — a fake would be testing the fake. The suite skips itself when no
 * database is listening, exactly like the rsvp one. */

import { and, eq, sql } from 'drizzle-orm';
import { afterAll, describe, expect, it } from 'vitest';

import { database, pool, schema } from '@/db/client';
import { createPerson } from '@/features/people/provision';
import { mergePersons } from '@/features/people/merge';
import { disableCascade, enableRestore } from '../disable';
import { factorsAtRisk, readUndo, splitMerge } from '../split';

const reachable = await (async () => {
  if (!process.env.DATABASE_URL) return false;
  try {
    await database().execute(sql`select 1`);
    return true;
  } catch {
    return false;
  }
})();

afterAll(async () => {
  if (reachable) await pool().end();
});

function tag(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
}

describe('the undo record', () => {
  it('is what a merge wrote about itself, or nothing at all', () => {
    expect(readUndo({ survivor: 1, absorbed: 2 })).toEqual({
      survivor: 1,
      absorbed: 2,
      moved_identities: [],
      dropped_identities: [],
      copied_relations: [],
      moved_resources: [],
      moved_sessions: [],
    });
    expect(readUndo(null)).toBe(null);
    expect(readUndo({ survivor: 1 })).toBe(null);
    expect(readUndo({ survivor: 0, absorbed: 2 })).toBe(null);
  });
});

describe('what a split would cost', () => {
  const merged = new Date('2026-09-01T00:00:00Z');
  const before = { identityId: 5, kind: 'password', createdAt: new Date('2026-08-01T00:00:00Z') };
  const after = { identityId: 5, kind: 'password', createdAt: new Date('2026-09-02T00:00:00Z') };
  const elsewhere = { identityId: 9, kind: 'password', createdAt: new Date('2026-09-02T00:00:00Z') };

  it('counts only factors gained after the merge on identities it takes back', () => {
    expect(factorsAtRisk([before, after, elsewhere], [5], merged)).toEqual([after]);
    expect(factorsAtRisk([before], [5], merged)).toEqual([]);
    expect(factorsAtRisk([after], [], merged)).toEqual([]);
  });
});

describe.skipIf(!reachable)('split', () => {
  it('puts back the identities, the edges and the person the merge took', async () => {
    const db = database();
    const seeded = await db.transaction(async (tx) => {
      const survivor = await createPerson(tx, { displayName: 'The Claimant' });
      const absorbed = await createPerson(tx, { displayName: 'The Card', held: true });
      const holder = await createPerson(tx, { displayName: 'The Holder' });
      const handle = tag('split');
      const identities = await tx
        .insert(schema.identity)
        .values({ personId: absorbed, source: 'handle', subject: handle })
        .returning({ id: schema.identity.id });
      await tx.insert(schema.relation).values({
        subjectKind: 'person',
        subjectId: holder,
        verb: 'contact',
        resourceKind: 'person',
        resourceId: absorbed,
      });
      return { survivor, absorbed, holder, identityId: identities[0]!.id, handle };
    });

    const merged = await db.transaction((tx) =>
      mergePersons(tx, {
        survivor: seeded.survivor,
        absorbed: seeded.absorbed,
        actorPersonId: seeded.survivor,
        method: 'match',
      }),
    );
    expect(merged).toBe(true);

    /* The handle moved and the holder now holds the survivor. */
    const afterMerge = await db
      .select({ personId: schema.identity.personId })
      .from(schema.identity)
      .where(eq(schema.identity.id, seeded.identityId));
    expect(afterMerge[0]?.personId).toBe(seeded.survivor);
    const copied = await db
      .select({ id: schema.relation.id })
      .from(schema.relation)
      .where(
        and(
          eq(schema.relation.subjectId, seeded.holder),
          eq(schema.relation.verb, 'contact'),
          eq(schema.relation.resourceId, seeded.survivor),
        ),
      );
    expect(copied).toHaveLength(1);

    const record = await db
      .select({ id: schema.audit.id })
      .from(schema.audit)
      .where(
        and(eq(schema.audit.command, 'confirm-match'), eq(schema.audit.targetId, seeded.survivor)),
      )
      .orderBy(sql`${schema.audit.id} desc`)
      .limit(1);
    const auditId = record[0]!.id;

    const outcome = await db.transaction((tx) =>
      splitMerge(tx, {
        auditId,
        actorPersonId: seeded.survivor,
        reason: 'they are two people',
        confirmed: false,
      }),
    );
    expect(outcome).toEqual({ ok: true, restored: 1 });

    /* Everything is where it was before the merge. */
    const restored = await db
      .select({ personId: schema.identity.personId })
      .from(schema.identity)
      .where(eq(schema.identity.id, seeded.identityId));
    expect(restored[0]?.personId).toBe(seeded.absorbed);

    const person = await db
      .select({ mergedInto: schema.person.mergedInto })
      .from(schema.person)
      .where(eq(schema.person.id, seeded.absorbed));
    expect(person[0]?.mergedInto).toBe(null);

    const party = await db
      .select({ disabledAt: schema.party.disabledAt })
      .from(schema.party)
      .where(eq(schema.party.id, seeded.absorbed));
    expect(party[0]?.disabledAt).toBe(null);

    const gone = await db
      .select({ id: schema.relation.id })
      .from(schema.relation)
      .where(
        and(
          eq(schema.relation.subjectId, seeded.holder),
          eq(schema.relation.verb, 'contact'),
          eq(schema.relation.resourceId, seeded.survivor),
        ),
      );
    expect(gone).toHaveLength(0);

    /* And splitting the same merge twice is splitting it once. */
    const again = await db.transaction((tx) =>
      splitMerge(tx, {
        auditId,
        actorPersonId: seeded.survivor,
        reason: 'again',
        confirmed: false,
      }),
    );
    expect(again).toEqual({ ok: false, reason: 'not-merged' });
  }, 10_000);

  it('refuses when the survivor gained a factor on an identity it would take back', async () => {
    const db = database();
    const seeded = await db.transaction(async (tx) => {
      const survivor = await createPerson(tx, { displayName: 'Gained A Door' });
      const absorbed = await createPerson(tx, { displayName: 'Their Card', held: true });
      const identities = await tx
        .insert(schema.identity)
        .values({ personId: absorbed, source: 'handle', subject: tag('risky') })
        .returning({ id: schema.identity.id });
      return { survivor, absorbed, identityId: identities[0]!.id };
    });

    await db.transaction((tx) =>
      mergePersons(tx, {
        survivor: seeded.survivor,
        absorbed: seeded.absorbed,
        actorPersonId: seeded.survivor,
        method: 'match',
      }),
    );
    /* A password set on that address after the two became one. */
    await db
      .insert(schema.factor)
      .values({ identityId: seeded.identityId, kind: 'password', secret: 'argon2id$…' });

    const record = await db
      .select({ id: schema.audit.id })
      .from(schema.audit)
      .where(
        and(eq(schema.audit.command, 'confirm-match'), eq(schema.audit.targetId, seeded.survivor)),
      )
      .orderBy(sql`${schema.audit.id} desc`)
      .limit(1);

    const refused = await db.transaction((tx) =>
      splitMerge(tx, {
        auditId: record[0]!.id,
        actorPersonId: seeded.survivor,
        reason: 'wrong',
        confirmed: false,
      }),
    );
    expect(refused).toEqual({ ok: false, reason: 'factors', at_risk: 1 });

    const forced = await db.transaction((tx) =>
      splitMerge(tx, {
        auditId: record[0]!.id,
        actorPersonId: seeded.survivor,
        reason: 'wrong, and I know',
        confirmed: true,
      }),
    );
    expect(forced).toEqual({ ok: true, restored: 1 });
  });
});

describe.skipIf(!reachable)('disable', () => {
  it('ends the sessions and stops the links, and enable puts the links back', async () => {
    const db = database();
    const seeded = await db.transaction(async (tx) => {
      const personId = await createPerson(tx, { displayName: 'Switched Off' });
      const sessions = await tx
        .insert(schema.session)
        .values({
          personId,
          tokenHash: tag('hash'),
          expiresAt: new Date(Date.now() + 86_400_000),
        })
        .returning({ id: schema.session.id });
      const links = await tx
        .insert(schema.link)
        .values({
          tokenHash: tag('link'),
          kind: 'claim',
          targetKind: 'person',
          targetId: personId,
          createdBy: personId,
        })
        .returning({ id: schema.link.id });
      return { personId, sessionId: sessions[0]!.id, linkId: links[0]!.id };
    });

    const cascade = await db.transaction(async (tx) => {
      const result = await disableCascade(tx, seeded.personId);
      await tx.insert(schema.audit).values({
        actorPersonId: seeded.personId,
        command: 'disable-party',
        targetKind: 'person',
        targetId: seeded.personId,
        payload: { links_revoked: result?.links ?? [] },
      });
      return result;
    });
    expect(cascade?.sessions).toBe(1);
    expect(cascade?.links).toEqual([seeded.linkId]);

    const off = await db
      .select({ revokedAt: schema.session.revokedAt })
      .from(schema.session)
      .where(eq(schema.session.id, seeded.sessionId));
    expect(off[0]?.revokedAt).not.toBe(null);
    const dead = await db
      .select({ revokedAt: schema.link.revokedAt })
      .from(schema.link)
      .where(eq(schema.link.id, seeded.linkId));
    expect(dead[0]?.revokedAt).not.toBe(null);

    /* Disabling twice is disabling once. */
    const again = await db.transaction((tx) => disableCascade(tx, seeded.personId));
    expect(again).toBe(null);

    const restored = await db.transaction((tx) => enableRestore(tx, seeded.personId));
    expect(restored).toBe(1);
    const alive = await db
      .select({ revokedAt: schema.link.revokedAt, disabled: schema.party.disabledAt })
      .from(schema.link)
      .innerJoin(schema.party, eq(schema.party.id, seeded.personId))
      .where(eq(schema.link.id, seeded.linkId));
    expect(alive[0]?.revokedAt).toBe(null);
    expect(alive[0]?.disabled).toBe(null);
  });
});
