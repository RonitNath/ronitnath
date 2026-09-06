/* The operator's half of the match queue.
 *
 * `src/features/people/matches.ts` is the seam: it writes the questions the
 * model noticed and settles the ones a member answers about themselves. This
 * reads the same rows for the pairs nobody can answer for themselves, and it
 * reads the merges that have already happened, because the only thing an
 * operator can do about a wrong one is take it back. */

import { and, desc, eq, inArray, isNull } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import { encodeId } from '@/lib/ids';

export interface QueuedMatch {
  publicId: string;
  signal: string;
  score: number;
  evidence: string | null;
  createdAt: Date;
  sides: { publicId: string; personPublicId: string; name: string; source: string; subject: string }[];
}

export async function listQueue(): Promise<QueuedMatch[]> {
  const db = database();
  const rows = await db
    .select({
      id: schema.match.id,
      identityA: schema.match.identityA,
      identityB: schema.match.identityB,
      signal: schema.match.signal,
      score: schema.match.score,
      evidence: schema.match.evidence,
      createdAt: schema.match.createdAt,
    })
    .from(schema.match)
    .where(eq(schema.match.status, 'proposed'))
    .orderBy(desc(schema.match.score), desc(schema.match.id))
    .limit(100);
  if (rows.length === 0) return [];

  const sides = await db
    .select({
      id: schema.identity.id,
      personId: schema.identity.personId,
      source: schema.identity.source,
      subject: schema.identity.subject,
      name: schema.person.displayName,
    })
    .from(schema.identity)
    .innerJoin(schema.person, eq(schema.person.id, schema.identity.personId))
    .where(inArray(schema.identity.id, rows.flatMap((row) => [row.identityA, row.identityB])));
  const byId = new Map(sides.map((row) => [row.id, row]));

  return rows.map((row) => ({
    publicId: encodeId('match', row.id),
    signal: row.signal,
    score: row.score,
    evidence: row.evidence,
    createdAt: row.createdAt,
    sides: [row.identityA, row.identityB].flatMap((identityId) => {
      const side = byId.get(identityId);
      if (!side) return [];
      return [
        {
          publicId: encodeId('identity', side.id),
          personPublicId: encodeId('person', side.personId),
          name: side.name,
          source: side.source,
          subject: side.subject,
        },
      ];
    }),
  }));
}

export interface MergeRecord {
  auditId: number;
  at: Date;
  method: string;
  survivor: { publicId: string; name: string };
  absorbed: { publicId: string; name: string };
}

/** Merges still in force, newest first: the rows a Split can undo. A merge
 *  whose absorbed person is no longer retired has already been taken back and
 *  is not offered again. */
export async function listMerges(): Promise<MergeRecord[]> {
  const db = database();
  const rows = await db
    .select({ id: schema.audit.id, at: schema.audit.at, payload: schema.audit.payload })
    .from(schema.audit)
    .where(eq(schema.audit.command, 'confirm-match'))
    .orderBy(desc(schema.audit.id))
    .limit(100);

  const parsed = rows.flatMap((row) => {
    const payload = row.payload as
      | { survivor?: number; absorbed?: number; method?: string }
      | null;
    if (!payload?.survivor || !payload.absorbed) return [];
    return [
      {
        auditId: row.id,
        at: row.at,
        method: payload.method ?? 'match',
        survivorId: payload.survivor,
        absorbedId: payload.absorbed,
      },
    ];
  });
  if (parsed.length === 0) return [];

  const people = await db
    .select({
      id: schema.person.id,
      name: schema.person.displayName,
      mergedInto: schema.person.mergedInto,
    })
    .from(schema.person)
    .where(
      inArray(schema.person.id, [
        ...parsed.map((row) => row.survivorId),
        ...parsed.map((row) => row.absorbedId),
      ]),
    );
  const byId = new Map(people.map((row) => [row.id, row]));

  return parsed.flatMap((row) => {
    const survivor = byId.get(row.survivorId);
    const absorbed = byId.get(row.absorbedId);
    if (!survivor || !absorbed || absorbed.mergedInto !== row.survivorId) return [];
    return [
      {
        auditId: row.auditId,
        at: row.at,
        method: row.method,
        survivor: { publicId: encodeId('person', survivor.id), name: survivor.name },
        absorbed: { publicId: encodeId('person', absorbed.id), name: absorbed.name },
      },
    ];
  });
}

/** The live people an operator may name in a proposal or a ruling. The bound
 *  is on the newest five hundred rather than on the alphabet: a list cut at
 *  the letter M is a list that silently loses whoever registered last. */
export async function livePeople(): Promise<{ publicId: string; name: string }[]> {
  const rows = await database()
    .select({ id: schema.person.id, name: schema.person.displayName })
    .from(schema.person)
    .innerJoin(schema.party, eq(schema.party.id, schema.person.id))
    .where(and(isNull(schema.person.mergedInto), isNull(schema.party.disabledAt)))
    .orderBy(desc(schema.person.id))
    .limit(500);
  return rows
    .map((row) => ({ publicId: encodeId('person', row.id), name: row.name }))
    .sort((one, two) => one.name.localeCompare(two.name));
}
