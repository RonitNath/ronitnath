/* What the operator's pages read. Reads only: every mutation is in
 * `actions.ts`, and nothing exported from here is a callable endpoint.
 *
 * Two rules run through all of it. Internal integer ids never leave the
 * server, so every row that reaches a page carries a public id or a name
 * (docs/design.md); and a secret is never selected at all — the factor rows
 * below name their kind and nothing else, so a password hash cannot reach a
 * page even by accident. */

import { and, desc, eq, gt, ilike, inArray, isNull, lt, or, sql, type SQL } from 'drizzle-orm';
import { alias } from 'drizzle-orm/pg-core';

import { database, schema } from '@/db/client';
import { OPERATOR_RESOURCE } from '@/features/auth/principal';
import { encodeId, tryDecodeId, type IdType } from '@/lib/ids';

export type PartyState = 'active' | 'disabled' | 'merged';

export interface PartyRow {
  id: number;
  publicId: string;
  kind: 'person' | 'organization' | 'service';
  name: string;
  handle: string | null;
  state: PartyState;
  held: boolean;
  createdAt: Date;
}

function stateOf(disabledAt: Date | null, mergedInto: number | null): PartyState {
  if (mergedInto !== null) return 'merged';
  return disabledAt === null ? 'active' : 'disabled';
}

function publicPartyId(kind: string, id: number): string {
  return encodeId(kind === 'organization' ? 'organization' : 'person', id);
}

/** Every party, newest first, optionally narrowed by one box: a name, a
 *  handle, an address, or a public id. A miss is an empty list, never a
 *  decline — an operator searching is not being probed. */
export async function listParties(query?: string): Promise<PartyRow[]> {
  const db = database();
  const clauses: SQL[] = [];
  const q = query?.trim();
  if (q) {
    const like = `%${q}%`;
    const byId = tryDecodeId('person', q) ?? tryDecodeId('organization', q);
    const subjects = db
      .select({ id: schema.identity.personId })
      .from(schema.identity)
      .where(ilike(schema.identity.subject, like));
    const found = or(
      ilike(schema.person.displayName, like),
      ilike(schema.organization.name, like),
      ilike(schema.organization.handle, like),
      inArray(schema.party.id, subjects),
      ...(byId === null ? [] : [eq(schema.party.id, byId)]),
    );
    if (found) clauses.push(found);
  }

  const rows = await db
    .select({
      id: schema.party.id,
      kind: schema.party.kind,
      disabledAt: schema.party.disabledAt,
      createdAt: schema.party.createdAt,
      displayName: schema.person.displayName,
      held: schema.person.held,
      mergedInto: schema.person.mergedInto,
      orgName: schema.organization.name,
      handle: schema.organization.handle,
    })
    .from(schema.party)
    .leftJoin(schema.person, eq(schema.person.id, schema.party.id))
    .leftJoin(schema.organization, eq(schema.organization.id, schema.party.id))
    .where(clauses.length > 0 ? clauses[0] : undefined)
    .orderBy(desc(schema.party.id))
    .limit(200);

  return rows.map((row) => ({
    id: row.id,
    publicId: publicPartyId(row.kind, row.id),
    kind: row.kind,
    name: row.displayName ?? row.orgName ?? '—',
    handle: row.handle,
    state: stateOf(row.disabledAt, row.mergedInto),
    held: row.held ?? false,
    createdAt: row.createdAt,
  }));
}

export interface IdentityDetail {
  publicId: string;
  source: string;
  subject: string;
  verifiedAt: Date | null;
  /* Kinds only. No secret is selected anywhere in this module. */
  factors: { kind: string; usedAt: Date | null; createdAt: Date }[];
}

export interface SessionDetail {
  publicId: string;
  personName: string;
  personPublicId: string;
  source: string;
  ip: string | null;
  userAgent: string | null;
  lastSeenAt: Date;
  expiresAt: Date;
  actingOperator: string | null;
}

export interface RelationDetail {
  verb: string;
  kind: string;
  label: string;
}

export interface AuditDetail {
  id: number;
  command: string;
  actorName: string | null;
  /* Which of the three kinds of caller this was. A signed-in member has a
   * name and a way in; a guest is a held person, named by somebody else and
   * holding nothing but a link; a row with no person at all was written by
   * the system — a script, a migration, the seed. One column cannot carry
   * "who did this" here without saying which kind of id it is carrying. */
  actorHeld: boolean | null;
  targetKind: string | null;
  /* Which row, not just which kind of row. Without it the feed says that an
   * event was published and leaves the operator to guess which event. */
  targetId: number | null;
  at: Date;
  payload: unknown;
}

export interface PartyDetail extends PartyRow {
  identities: IdentityDetail[];
  sessions: SessionDetail[];
  held_by: { publicId: string; name: string }[];
  holds: { publicId: string; name: string }[];
  relationsHeld: RelationDetail[];
  relationsGranted: RelationDetail[];
  audit: AuditDetail[];
  mergedIntoName: string | null;
  absorbed: { publicId: string; name: string; auditId: number | null }[];
}

const ID_FOR: Record<string, IdType> = {
  person: 'person',
  organization: 'organization',
  group: 'group',
  document: 'document',
  event: 'event',
  link: 'link',
  session: 'session',
  identity: 'identity',
  match: 'match',
};

/** A relation's other half, said in public. `platform:*` is the one resource
 *  with no row and no id, and it is the one this whole tier turns on. */
function label(kind: string, id: number, names: Map<string, string>): string {
  if (kind === OPERATOR_RESOURCE.kind) return 'platform:*';
  const named = names.get(`${kind}:${id}`);
  if (named) return named;
  const type = ID_FOR[kind];
  return type ? encodeId(type, id) : `${kind}:${id}`;
}

async function nameParties(ids: number[]): Promise<Map<string, string>> {
  const names = new Map<string, string>();
  if (ids.length === 0) return names;
  const db = database();
  const people = await db
    .select({ id: schema.person.id, name: schema.person.displayName })
    .from(schema.person)
    .where(inArray(schema.person.id, ids));
  for (const row of people) names.set(`person:${row.id}`, row.name);
  const orgs = await db
    .select({ id: schema.organization.id, name: schema.organization.name })
    .from(schema.organization)
    .where(inArray(schema.organization.id, ids));
  for (const row of orgs) names.set(`organization:${row.id}`, row.name);
  return names;
}

export async function partyDetail(id: number): Promise<PartyDetail | null> {
  const db = database();
  const rows = await db
    .select({
      id: schema.party.id,
      kind: schema.party.kind,
      disabledAt: schema.party.disabledAt,
      createdAt: schema.party.createdAt,
      displayName: schema.person.displayName,
      held: schema.person.held,
      mergedInto: schema.person.mergedInto,
      orgName: schema.organization.name,
      handle: schema.organization.handle,
    })
    .from(schema.party)
    .leftJoin(schema.person, eq(schema.person.id, schema.party.id))
    .leftJoin(schema.organization, eq(schema.organization.id, schema.party.id))
    .where(eq(schema.party.id, id))
    .limit(1);
  const row = rows[0];
  if (!row) return null;

  const identities = await db
    .select({
      id: schema.identity.id,
      source: schema.identity.source,
      subject: schema.identity.subject,
      verifiedAt: schema.identity.verifiedAt,
    })
    .from(schema.identity)
    .where(eq(schema.identity.personId, id))
    .orderBy(schema.identity.id);
  const factors =
    identities.length === 0
      ? []
      : await db
          .select({
            identityId: schema.factor.identityId,
            kind: schema.factor.kind,
            usedAt: schema.factor.usedAt,
            createdAt: schema.factor.createdAt,
          })
          .from(schema.factor)
          .where(inArray(schema.factor.identityId, identities.map((one) => one.id)));

  const sessions = await listSessions({ personId: id });

  const held = await db
    .select({
      subjectKind: schema.relation.subjectKind,
      subjectId: schema.relation.subjectId,
      verb: schema.relation.verb,
      resourceKind: schema.relation.resourceKind,
      resourceId: schema.relation.resourceId,
    })
    .from(schema.relation)
    .where(
      or(
        and(inArray(schema.relation.subjectKind, ['person', 'organization']), eq(schema.relation.subjectId, id)),
        and(inArray(schema.relation.resourceKind, ['person', 'organization']), eq(schema.relation.resourceId, id)),
      ),
    );
  const names = await nameParties([
    ...held.map((one) => one.subjectId),
    ...held.map((one) => one.resourceId),
  ]);

  const audit = await db
    .select({
      id: schema.audit.id,
      command: schema.audit.command,
      targetKind: schema.audit.targetKind,
      targetId: schema.audit.targetId,
      at: schema.audit.at,
      payload: schema.audit.payload,
      actorName: schema.person.displayName,
      actorHeld: schema.person.held,
    })
    .from(schema.audit)
    .leftJoin(schema.person, eq(schema.person.id, schema.audit.actorPersonId))
    .where(
      or(
        eq(schema.audit.actorPersonId, id),
        and(eq(schema.audit.targetKind, 'person'), eq(schema.audit.targetId, id)),
      ),
    )
    .orderBy(desc(schema.audit.id))
    .limit(50);

  const absorbedRows = await db
    .select({ id: schema.person.id, name: schema.person.displayName })
    .from(schema.person)
    .where(eq(schema.person.mergedInto, id));
  const merges = await db
    .select({ id: schema.audit.id, payload: schema.audit.payload })
    .from(schema.audit)
    .where(and(eq(schema.audit.command, 'confirm-match'), eq(schema.audit.targetId, id)))
    .orderBy(desc(schema.audit.id));
  const mergeOf = new Map<number, number>();
  for (const one of merges) {
    const payload = one.payload as { absorbed?: number } | null;
    if (payload?.absorbed && !mergeOf.has(payload.absorbed)) mergeOf.set(payload.absorbed, one.id);
  }

  const mergedIntoName =
    row.mergedInto === null
      ? null
      : ((
          await db
            .select({ name: schema.person.displayName })
            .from(schema.person)
            .where(eq(schema.person.id, row.mergedInto))
            .limit(1)
        )[0]?.name ?? null);

  return {
    id: row.id,
    publicId: publicPartyId(row.kind, row.id),
    kind: row.kind,
    name: row.displayName ?? row.orgName ?? '—',
    handle: row.handle,
    state: stateOf(row.disabledAt, row.mergedInto),
    held: row.held ?? false,
    createdAt: row.createdAt,
    identities: identities.map((one) => ({
      publicId: encodeId('identity', one.id),
      source: one.source,
      subject: one.subject,
      verifiedAt: one.verifiedAt,
      factors: factors
        .filter((factor) => factor.identityId === one.id)
        .map(({ kind, usedAt, createdAt }) => ({ kind, usedAt, createdAt })),
    })),
    sessions,
    held_by: held
      .filter((one) => one.verb === 'contact' && one.resourceId === id)
      .map((one) => ({
        publicId: encodeId('person', one.subjectId),
        name: names.get(`person:${one.subjectId}`) ?? '—',
      })),
    holds: held
      .filter((one) => one.verb === 'contact' && one.subjectId === id)
      .map((one) => ({
        publicId: encodeId('person', one.resourceId),
        name: names.get(`person:${one.resourceId}`) ?? '—',
      })),
    relationsHeld: held
      .filter((one) => one.subjectId === id && one.verb !== 'contact')
      .map((one) => ({
        verb: one.verb,
        kind: one.resourceKind,
        label: label(one.resourceKind, one.resourceId, names),
      })),
    relationsGranted: held
      .filter((one) => one.resourceId === id && one.verb !== 'contact')
      .map((one) => ({
        verb: one.verb,
        kind: one.subjectKind,
        label: label(one.subjectKind, one.subjectId, names),
      })),
    audit,
    mergedIntoName,
    absorbed: absorbedRows.map((one) => ({
      publicId: encodeId('person', one.id),
      name: one.name,
      auditId: mergeOf.get(one.id) ?? null,
    })),
  };
}

/** Live sessions, deployment-wide or for one person. */
export async function listSessions(filter?: { personId?: number }): Promise<SessionDetail[]> {
  const holder = alias(schema.person, 'holder');
  const operator = alias(schema.person, 'operator_person');
  const rows = await database()
    .select({
      id: schema.session.id,
      personId: schema.session.personId,
      personName: holder.displayName,
      source: schema.session.source,
      ip: schema.session.ip,
      userAgent: schema.session.userAgent,
      lastSeenAt: schema.session.lastSeenAt,
      expiresAt: schema.session.expiresAt,
      actingOperator: operator.displayName,
    })
    .from(schema.session)
    .innerJoin(holder, eq(holder.id, schema.session.personId))
    .leftJoin(operator, eq(operator.id, schema.session.actingOperatorId))
    .where(
      and(
        isNull(schema.session.revokedAt),
        gt(schema.session.expiresAt, sql`now()`),
        ...(filter?.personId ? [eq(schema.session.personId, filter.personId)] : []),
      ),
    )
    .orderBy(desc(schema.session.lastSeenAt))
    .limit(200);

  return rows.map((row) => ({
    publicId: encodeId('session', row.id),
    personName: row.personName,
    personPublicId: encodeId('person', row.personId),
    source: row.source,
    ip: row.ip,
    userAgent: row.userAgent,
    lastSeenAt: row.lastSeenAt,
    expiresAt: row.expiresAt,
    actingOperator: row.actingOperator,
  }));
}

export const AUDIT_PAGE = 50;

/** The audit table, newest first, read by id.
 *
 *  Newest first because that is the question an operator has: what has this
 *  deployment just done. Read by id rather than by offset because the table is
 *  append-only — new rows land above the page rather than inside it, so a
 *  cursor is the oldest id the previous page showed and nothing can slide
 *  underneath it. A page numbered by offset would show the same row twice the
 *  moment a command ran between two clicks. */
export async function readAudit(filter: {
  before?: number;
  actor?: number;
  command?: string;
  targetKind?: string;
}): Promise<{ rows: AuditDetail[]; next: number | null }> {
  const clauses: SQL[] = [];
  if (filter.before) clauses.push(lt(schema.audit.id, filter.before));
  if (filter.actor) clauses.push(eq(schema.audit.actorPersonId, filter.actor));
  if (filter.command) clauses.push(eq(schema.audit.command, filter.command));
  if (filter.targetKind) clauses.push(eq(schema.audit.targetKind, filter.targetKind));

  const rows = await database()
    .select({
      id: schema.audit.id,
      command: schema.audit.command,
      targetKind: schema.audit.targetKind,
      targetId: schema.audit.targetId,
      at: schema.audit.at,
      payload: schema.audit.payload,
      actorName: schema.person.displayName,
      actorHeld: schema.person.held,
    })
    .from(schema.audit)
    .leftJoin(schema.person, eq(schema.person.id, schema.audit.actorPersonId))
    .where(clauses.length > 0 ? and(...clauses) : undefined)
    .orderBy(desc(schema.audit.id))
    .limit(AUDIT_PAGE + 1);

  const page = rows.slice(0, AUDIT_PAGE);
  return { rows: page, next: rows.length > AUDIT_PAGE ? (page.at(-1)?.id ?? null) : null };
}

/** Every distinct command the feed holds, for the filter. */
export async function auditCommands(): Promise<string[]> {
  const rows = await database()
    .selectDistinct({ command: schema.audit.command })
    .from(schema.audit)
    .orderBy(schema.audit.command);
  return rows.map((row) => row.command);
}
