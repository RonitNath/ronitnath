/* allows(ctx, want). One check, and every feature ends in it.
 *
 * A person is never only themselves. They are also every group they are in,
 * every group that contains one of those, and every organization they belong
 * to — and a relation written against any of those subjects reaches them.
 * `subjectsOf` is that expansion, `allows` is one indexed query over it, and
 * the last clause is always "…or a platform operator" (docs/plan.md §Model).
 *
 * Two vocabularies share one shape. `member < admin < owner` is a role on an
 * organization or a group; `viewer < commenter < editor` is a level on a
 * document, under the document's owner. Nesting is done here, in code: a row
 * saying `editor` answers a question asking for `viewer`, and no command has
 * to remember to look for three verbs.
 *
 * A refusal never says which clause it failed. The caller turns false into
 * the same 404 that a row which does not exist gets, because the difference
 * is exactly the fact a stranger would like to learn. */

import { and, eq, inArray, or, type SQL } from 'drizzle-orm';

import { schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';

export type SubjectKind = 'person' | 'group' | 'organization';
export interface Subject {
  kind: SubjectKind;
  id: number;
}

export interface Actor {
  personId: number;
  isOperator: boolean;
}

export type Role = 'member' | 'admin' | 'owner';
export type Level = 'viewer' | 'commenter' | 'editor';

export const ROLES: readonly Role[] = ['member', 'admin', 'owner'];
export const LEVELS: readonly Level[] = ['viewer', 'commenter', 'editor'];

const ROLE_RANK: Record<string, number> = { member: 1, admin: 2, owner: 3 };
/* `owner` outranks every level a share can grant: the party a document
 * belongs to is not one of its readers. */
const LEVEL_RANK: Record<string, number> = { viewer: 1, commenter: 2, editor: 3, owner: 4 };

export type ResourceKind = 'organization' | 'group' | 'document';

export interface Want {
  on: ResourceKind;
  id: number;
  need: Role | Level | 'owner';
}

function ranks(on: ResourceKind): Record<string, number> {
  return on === 'document' ? LEVEL_RANK : ROLE_RANK;
}

export function rankOf(on: ResourceKind, verb: string): number {
  return ranks(on)[verb] ?? 0;
}

export function isRole(value: string): value is Role {
  return ROLES.includes(value as Role);
}

export function isLevel(value: string): value is Level {
  return LEVELS.includes(value as Level);
}

/* ---------------------------------------------------------------- subjects */

export interface MembershipEdge {
  subject: Subject;
  /* A group or an organization: the only two things a subject is *in*. */
  resource: Subject;
}

const MAX_DEPTH = 16;

function key(subject: Subject): string {
  return `${subject.kind}:${subject.id}`;
}

/** The transitive set {seed, its groups, the groups containing those, the
 *  organizations it belongs to}. Pure, so nesting and cycles can be stated
 *  without a database — and a cycle simply stops the walk, because a subject
 *  already visited is not visited again. */
export function expandSubjects(seed: Subject, edges: readonly MembershipEdge[]): Subject[] {
  const out: Subject[] = [seed];
  const seen = new Set([key(seed)]);
  let frontier: Subject[] = [seed];

  for (let depth = 0; depth < MAX_DEPTH && frontier.length > 0; depth += 1) {
    const next: Subject[] = [];
    for (const edge of edges) {
      if (!frontier.some((row) => row.kind === edge.subject.kind && row.id === edge.subject.id)) {
        continue;
      }
      if (seen.has(key(edge.resource))) continue;
      seen.add(key(edge.resource));
      out.push(edge.resource);
      next.push(edge.resource);
    }
    frontier = next;
  }
  return out;
}

/** Whether putting `child` inside `parent` would close a loop: it would, iff
 *  the parent is already inside the child. */
export function wouldCycle(
  edges: readonly MembershipEdge[],
  parent: Subject,
  child: Subject,
): boolean {
  if (parent.kind === child.kind && parent.id === child.id) return true;
  return expandSubjects(parent, edges).some(
    (row) => row.kind === child.kind && row.id === child.id,
  );
}

/** Every membership edge in the deployment. The graph is people's circles and
 *  a handful of organizations — small, and read whole so that the walk above
 *  stays one pure function rather than a recursive query nobody can test. */
export async function membershipEdges(tx: Transaction): Promise<MembershipEdge[]> {
  const rows = await tx
    .select({
      subjectKind: schema.relation.subjectKind,
      subjectId: schema.relation.subjectId,
      resourceKind: schema.relation.resourceKind,
      resourceId: schema.relation.resourceId,
    })
    .from(schema.relation)
    .where(
      and(
        inArray(schema.relation.resourceKind, ['group', 'organization']),
        inArray(schema.relation.verb, [...ROLES]),
      ),
    );
  return rows
    .filter((row) => row.subjectKind === 'person' || row.subjectKind === 'group')
    .map((row) => ({
      subject: { kind: row.subjectKind as SubjectKind, id: row.subjectId },
      resource: { kind: row.resourceKind as SubjectKind, id: row.resourceId },
    }));
}

/** Everything a relation could name this person by. */
export async function subjectsOf(tx: Transaction, personId: number): Promise<Subject[]> {
  return expandSubjects({ kind: 'person', id: personId }, await membershipEdges(tx));
}

/* ------------------------------------------------------------------ allows */

/** The `(subject_kind, subject_id) in (…)` half of every relation query. */
export function subjectClause(subjects: readonly Subject[]): SQL | undefined {
  const clauses: SQL[] = [];
  for (const kind of ['person', 'group', 'organization'] as const) {
    const ids = subjects.filter((row) => row.kind === kind).map((row) => row.id);
    if (ids.length === 0) continue;
    clauses.push(
      and(eq(schema.relation.subjectKind, kind), inArray(schema.relation.subjectId, ids))!,
    );
  }
  return clauses.length === 1 ? clauses[0] : or(...clauses);
}

/** The party a resource belongs to, from the registry every command writes.
 *  Organizations are not registered: an organization belongs to nobody but
 *  the people holding `owner` on it. */
async function ownerParty(tx: Transaction, want: Want): Promise<number | null> {
  if (want.on === 'organization') return null;
  const rows = await tx
    .select({ ownerPartyId: schema.resource.ownerPartyId })
    .from(schema.resource)
    .where(and(eq(schema.resource.kind, want.on), eq(schema.resource.refId, want.id)))
    .limit(1);
  return rows[0]?.ownerPartyId ?? null;
}

/** What the database found: who the actor is, who the thing belongs to, and
 *  every verb the two have between them. */
export interface Held {
  subjects: readonly Subject[];
  /* The party the resource belongs to, or null when it belongs to nobody. */
  ownerParty: number | null;
  verbs: readonly string[];
}

/** The decision itself, away from the queries: what a rank is worth, what
 *  ownership is worth, and the operator clause at the end. */
export function decide(held: Held, want: Want, isOperator: boolean): boolean {
  return rankOf(want.on, best(held, want.on) ?? '') >= rankOf(want.on, want.need) || isOperator;
}

/** The best verb the actor holds, or null. The party a thing belongs to holds
 *  every rank over it: an organization that owns a document *is* its owner,
 *  and so is everyone the organization is. Groups are not parties, so a group
 *  subject can never match an owning party by its id. */
export function best(held: Held, on: ResourceKind): string | null {
  if (
    held.ownerParty !== null &&
    held.subjects.some((row) => row.kind !== 'group' && row.id === held.ownerParty)
  ) {
    return 'owner';
  }
  let found: string | null = null;
  for (const verb of held.verbs) {
    if (rankOf(on, verb) === 0) continue;
    if (found === null || rankOf(on, verb) > rankOf(on, found)) found = verb;
  }
  return found;
}

async function heldBy(tx: Transaction, personId: number, want: Want): Promise<Held> {
  const subjects = await subjectsOf(tx, personId);
  const ownerParty_ = await ownerParty(tx, want);
  const where = subjectClause(subjects);
  if (where === undefined) return { subjects, ownerParty: ownerParty_, verbs: [] };
  const rows = await tx
    .select({ verb: schema.relation.verb })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.resourceKind, want.on),
        eq(schema.relation.resourceId, want.id),
        where,
      ),
    );
  return { subjects, ownerParty: ownerParty_, verbs: rows.map((row) => row.verb) };
}

export async function allows(tx: Transaction, actor: Actor, want: Want): Promise<boolean> {
  return decide(await heldBy(tx, actor.personId, want), want, actor.isOperator);
}

/** The best verb this actor holds over a resource, for a page that has to
 *  print the word. Null when they hold none — which a page reads as 404. */
export async function heldRank(
  tx: Transaction,
  actor: Actor,
  on: ResourceKind,
  id: number,
): Promise<string | null> {
  const held = await heldBy(tx, actor.personId, { on, id, need: 'owner' });
  const found = best(held, on);
  if (found !== null) return found;
  return actor.isOperator ? 'owner' : null;
}

/** Write a role or level edge. One spelling per subject and resource: a
 *  change of rank replaces the row rather than adding a second one. */
export async function grant(
  tx: Transaction,
  input: { subject: Subject; verb: string; on: ResourceKind; id: number },
): Promise<void> {
  await revoke(tx, { subject: input.subject, on: input.on, id: input.id });
  await tx
    .insert(schema.relation)
    .values({
      subjectKind: input.subject.kind,
      subjectId: input.subject.id,
      verb: input.verb,
      resourceKind: input.on,
      resourceId: input.id,
    })
    .onConflictDoNothing();
}

export async function revoke(
  tx: Transaction,
  input: { subject: Subject; on: ResourceKind; id: number },
): Promise<void> {
  const verbs = input.on === 'document' ? [...LEVELS] : [...ROLES];
  await tx
    .delete(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, input.subject.kind),
        eq(schema.relation.subjectId, input.subject.id),
        inArray(schema.relation.verb, verbs),
        eq(schema.relation.resourceKind, input.on),
        eq(schema.relation.resourceId, input.id),
      ),
    );
}

/** The people who hold `owner` over something directly. What "the last owner
 *  cannot leave" counts. */
export async function ownerPersonIds(
  tx: Transaction,
  on: ResourceKind,
  id: number,
): Promise<number[]> {
  const rows = await tx
    .select({ id: schema.relation.subjectId })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.verb, 'owner'),
        eq(schema.relation.resourceKind, on),
        eq(schema.relation.resourceId, id),
      ),
    );
  return rows.map((row) => row.id);
}

/** Whether removing this person would leave the thing with nobody to own it.
 *  An organization with no owner is an organization nobody can fix. */
export function isLastOwner(owners: readonly number[], personId: number): boolean {
  return owners.includes(personId) && owners.length <= 1;
}

/** Register a resource so that `allows` can find who it belongs to. */
export async function registerResource(
  tx: Transaction,
  input: { kind: ResourceKind | 'event'; id: number; ownerPartyId: number },
): Promise<void> {
  await tx
    .insert(schema.resource)
    .values({ kind: input.kind, refId: input.id, ownerPartyId: input.ownerPartyId })
    .onConflictDoUpdate({
      target: [schema.resource.kind, schema.resource.refId],
      set: { ownerPartyId: input.ownerPartyId },
    });
}
