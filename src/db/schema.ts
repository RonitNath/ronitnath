/* The kernel model, simplified for Postgres (docs/plan.md §Model). Internal
 * ids are integers and never leave the server; the public id codec in
 * src/lib/ids.ts is the only thing that turns one into a string. Feature
 * modules take their slice of this file as they land. */

import { sql } from 'drizzle-orm';
import {
  type AnyPgColumn,
  bigserial,
  boolean,
  check,
  index,
  integer,
  jsonb,
  pgEnum,
  pgTable,
  serial,
  smallint,
  text,
  timestamp,
  uniqueIndex,
} from 'drizzle-orm/pg-core';

const now = () => timestamp('created_at', { withTimezone: true }).notNull().defaultNow();

export const partyKind = pgEnum('party_kind', ['person', 'organization', 'service']);
/* `handle` is the third kind of subject and the only one that is not a door:
 * the thing a member typed when they held someone — an address, a number, or
 * a bare name — normalised so that a later verified address can be compared
 * against it. It authenticates nobody. */
export const identitySource = pgEnum('identity_source', ['local', 'oidc', 'handle']);
export const factorKind = pgEnum('factor_kind', ['email', 'password', 'oidc']);
export const linkKind = pgEnum('link_kind', [
  'verify_email',
  'reset_password',
  'invitation',
  'claim',
  'event_personal',
  'event_open',
]);
export const inviteKind = pgEnum('invite_kind', ['personal', 'open']);
export const rsvpResponse = pgEnum('rsvp_response', ['yes', 'maybe', 'no']);
/* Why two identities might be one person, and what was decided about it.
 * A score orders the operator's queue (R6) and never merges anything by
 * itself — a merge is always somebody's answer. */
export const matchSignal = pgEnum('match_signal', ['verified_email', 'claimed_link']);
export const matchStatus = pgEnum('match_status', ['proposed', 'confirmed', 'rejected']);

/* Every addressable subject is a party; person and organization extend it by
 * sharing its id, so a relation can name either without a union column. */
export const party = pgTable('party', {
  id: serial('id').primaryKey(),
  kind: partyKind('kind').notNull(),
  createdAt: now(),
  disabledAt: timestamp('disabled_at', { withTimezone: true }),
});

export const person = pgTable(
  'person',
  {
    id: integer('id')
      .primaryKey()
      .references(() => party.id, { onDelete: 'cascade' }),
    displayName: text('display_name').notNull(),
    /* A held person has no way to sign in yet; a claim link turns that around. */
    held: boolean('held').notNull().default(false),
    /* Retirement. A merged person keeps every row it had — that record is what
     * an operator would need to undo it — and stops resolving: whoever asks
     * for this person is answered with the one it was folded into. */
    mergedInto: integer('merged_into').references((): AnyPgColumn => person.id, {
      onDelete: 'set null',
    }),
    createdAt: now(),
  },
  (t) => [index('person_merged_into_idx').on(t.mergedInto)],
);

export const organization = pgTable(
  'organization',
  {
    id: integer('id')
      .primaryKey()
      .references(() => party.id, { onDelete: 'cascade' }),
    handle: text('handle').notNull(),
    name: text('name').notNull(),
    createdAt: now(),
  },
  (t) => [uniqueIndex('organization_handle_key').on(t.handle)],
);

export const identity = pgTable(
  'identity',
  {
    id: serial('id').primaryKey(),
    personId: integer('person_id')
      .notNull()
      .references(() => person.id, { onDelete: 'cascade' }),
    source: identitySource('source').notNull(),
    subject: text('subject').notNull(),
    verifiedAt: timestamp('verified_at', { withTimezone: true }),
    createdAt: now(),
  },
  (t) => [
    /* A door has to be unique deployment-wide or it is not a door. A handle
     * is not a door: it is one member's note about how to reach somebody, and
     * two members may well hold the same address. The predicate names the two
     * sign-in sources rather than excluding `handle`, so that the migration
     * that adds the enum value can create this index in the same
     * transaction. */
    uniqueIndex('identity_source_subject_key')
      .on(t.source, t.subject)
      .where(sql`source in ('local', 'oidc')`),
    /* What ProposeMatch reads: every handle that spells this address. */
    index('identity_subject_idx').on(t.source, t.subject),
    index('identity_person_idx').on(t.personId),
  ],
);

export const factor = pgTable(
  'factor',
  {
    id: serial('id').primaryKey(),
    identityId: integer('identity_id')
      .notNull()
      .references(() => identity.id, { onDelete: 'cascade' }),
    kind: factorKind('kind').notNull(),
    /* Password hashes only, algorithm-tagged; nothing here renders to a page. */
    secret: text('secret'),
    meta: jsonb('meta'),
    usedAt: timestamp('used_at', { withTimezone: true }),
    createdAt: now(),
  },
  (t) => [uniqueIndex('factor_identity_kind_key').on(t.identityId, t.kind)],
);

export const session = pgTable(
  'session',
  {
    id: serial('id').primaryKey(),
    personId: integer('person_id')
      .notNull()
      .references(() => person.id, { onDelete: 'cascade' }),
    tokenHash: text('token_hash').notNull(),
    /* Which door this session came through. An OIDC session ends at the OP as
     * well as here, and the id_token it was minted with is the hint that
     * end_session needs; a local session has neither. */
    source: identitySource('source').notNull().default('local'),
    oidcIdToken: text('oidc_id_token'),
    userAgent: text('user_agent'),
    ip: text('ip'),
    lastSeenAt: timestamp('last_seen_at', { withTimezone: true }).notNull().defaultNow(),
    expiresAt: timestamp('expires_at', { withTimezone: true }).notNull(),
    revokedAt: timestamp('revoked_at', { withTimezone: true }),
    createdAt: now(),
  },
  (t) => [
    uniqueIndex('session_token_hash_key').on(t.tokenHash),
    index('session_person_idx').on(t.personId),
  ],
);

/* One table for every single-use URL the site hands out. */
export const link = pgTable(
  'link',
  {
    id: serial('id').primaryKey(),
    tokenHash: text('token_hash').notNull(),
    kind: linkKind('kind').notNull(),
    targetKind: text('target_kind').notNull(),
    targetId: integer('target_id').notNull(),
    createdBy: integer('created_by').references(() => person.id, { onDelete: 'set null' }),
    expiresAt: timestamp('expires_at', { withTimezone: true }),
    /* A one-shot link is spent once (`used_at`). An invitation is a longer
     * story that the member who sent it can watch: opened, claimed, or taken
     * back before either. */
    usedAt: timestamp('used_at', { withTimezone: true }),
    openedAt: timestamp('opened_at', { withTimezone: true }),
    claimedAt: timestamp('claimed_at', { withTimezone: true }),
    claimedBy: integer('claimed_by').references(() => person.id, { onDelete: 'set null' }),
    revokedAt: timestamp('revoked_at', { withTimezone: true }),
    createdAt: now(),
  },
  (t) => [
    uniqueIndex('link_token_hash_key').on(t.tokenHash),
    index('link_target_idx').on(t.targetKind, t.targetId),
    index('link_created_by_idx').on(t.createdBy),
  ],
);

export const group = pgTable(
  'group',
  {
    id: serial('id').primaryKey(),
    organizationId: integer('organization_id')
      .notNull()
      .references(() => organization.id, { onDelete: 'cascade' }),
    handle: text('handle').notNull(),
    name: text('name').notNull(),
    createdAt: now(),
  },
  (t) => [uniqueIndex('group_org_handle_key').on(t.organizationId, t.handle)],
);

/* The registry that lets a relation point at anything without a foreign key
 * per kind: (kind, ref_id) is the row a relation's resource half names. */
export const resource = pgTable(
  'resource',
  {
    id: serial('id').primaryKey(),
    kind: text('kind').notNull(),
    refId: integer('ref_id').notNull(),
    ownerPartyId: integer('owner_party_id').references(() => party.id, { onDelete: 'cascade' }),
    createdAt: now(),
  },
  (t) => [
    uniqueIndex('resource_kind_ref_key').on(t.kind, t.refId),
    index('resource_owner_idx').on(t.ownerPartyId),
  ],
);

/* subject verb resource. The whole authorisation model is rows in here. */
export const relation = pgTable(
  'relation',
  {
    id: bigserial('id', { mode: 'number' }).primaryKey(),
    subjectKind: text('subject_kind').notNull(),
    subjectId: integer('subject_id').notNull(),
    verb: text('verb').notNull(),
    resourceKind: text('resource_kind').notNull(),
    resourceId: integer('resource_id').notNull(),
    createdAt: now(),
  },
  (t) => [
    uniqueIndex('relation_edge_key').on(
      t.subjectKind,
      t.subjectId,
      t.verb,
      t.resourceKind,
      t.resourceId,
    ),
    index('relation_resource_idx').on(t.resourceKind, t.resourceId),
    index('relation_subject_idx').on(t.subjectKind, t.subjectId),
  ],
);

export const document = pgTable(
  'document',
  {
    id: serial('id').primaryKey(),
    ownerPartyId: integer('owner_party_id')
      .notNull()
      .references(() => party.id, { onDelete: 'cascade' }),
    title: text('title').notNull(),
    body: text('body').notNull().default(''),
    updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
    createdAt: now(),
  },
  (t) => [index('document_owner_idx').on(t.ownerPartyId)],
);

export const event = pgTable(
  'event',
  {
    id: serial('id').primaryKey(),
    hostPersonId: integer('host_person_id')
      .notNull()
      .references(() => person.id, { onDelete: 'cascade' }),
    slug: text('slug').notNull(),
    title: text('title').notNull(),
    summary: text('summary'),
    startsAt: timestamp('starts_at', { withTimezone: true }).notNull(),
    endsAt: timestamp('ends_at', { withTimezone: true }),
    location: text('location'),
    /* Held back until a guest says yes. */
    address: text('address'),
    publishedAt: timestamp('published_at', { withTimezone: true }),
    createdAt: now(),
  },
  (t) => [uniqueIndex('event_slug_key').on(t.slug), index('event_host_idx').on(t.hostPersonId)],
);

export const eventInvite = pgTable(
  'event_invite',
  {
    id: serial('id').primaryKey(),
    eventId: integer('event_id')
      .notNull()
      .references(() => event.id, { onDelete: 'cascade' }),
    personId: integer('person_id').references(() => person.id, { onDelete: 'cascade' }),
    kind: inviteKind('kind').notNull(),
    plusOneAllowed: boolean('plus_one_allowed').notNull().default(false),
    createdAt: now(),
  },
  (t) => [
    uniqueIndex('event_invite_person_key').on(t.eventId, t.personId),
    index('event_invite_event_idx').on(t.eventId),
  ],
);

export const rsvp = pgTable(
  'rsvp',
  {
    id: serial('id').primaryKey(),
    eventId: integer('event_id')
      .notNull()
      .references(() => event.id, { onDelete: 'cascade' }),
    personId: integer('person_id')
      .notNull()
      .references(() => person.id, { onDelete: 'cascade' }),
    response: rsvpResponse('response').notNull(),
    plusOne: smallint('plus_one').notNull().default(0),
    note: text('note'),
    answeredAt: timestamp('answered_at', { withTimezone: true }).notNull().defaultNow(),
    createdAt: now(),
  },
  (t) => [
    uniqueIndex('rsvp_event_person_key').on(t.eventId, t.personId),
    index('rsvp_event_idx').on(t.eventId),
  ],
);

/* Two identities that might be one person. A row is written by whatever
 * noticed — a verified address that equals somebody's handle, a claimed
 * invitation — and settled by an answer: the member's, on `/app`, or the
 * operator's in R6. Nothing merges on a score.
 *
 * `identity_a < identity_b` is enforced rather than conventional, so the pair
 * has one spelling and the unique index means what it says. */
export const match = pgTable(
  'match',
  {
    id: serial('id').primaryKey(),
    identityA: integer('identity_a')
      .notNull()
      .references(() => identity.id, { onDelete: 'cascade' }),
    identityB: integer('identity_b')
      .notNull()
      .references(() => identity.id, { onDelete: 'cascade' }),
    signal: matchSignal('signal').notNull(),
    /* Queue order for the operator's list, in hundredths. Never a threshold. */
    score: smallint('score').notNull().default(0),
    status: matchStatus('status').notNull().default('proposed'),
    evidence: text('evidence'),
    decidedAt: timestamp('decided_at', { withTimezone: true }),
    decidedBy: integer('decided_by').references(() => person.id, { onDelete: 'set null' }),
    createdAt: now(),
  },
  (t) => [
    uniqueIndex('match_pair_signal_key').on(t.identityA, t.identityB, t.signal),
    index('match_status_idx').on(t.status),
    index('match_identity_b_idx').on(t.identityB),
    check('match_ordered_pair', sql`${t.identityA} < ${t.identityB}`),
  ],
);

/* Sign-in and reset attempts, counted per (scope, key) in a fixed window.
 * Postgres is the only store this deployment has, and a counter row that the
 * command's own transaction touches is one fewer thing to run and to lose. */
export const authThrottle = pgTable(
  'auth_throttle',
  {
    id: serial('id').primaryKey(),
    scope: text('scope').notNull(),
    key: text('key').notNull(),
    windowStartedAt: timestamp('window_started_at', { withTimezone: true }).notNull().defaultNow(),
    count: integer('count').notNull().default(0),
  },
  (t) => [uniqueIndex('auth_throttle_scope_key').on(t.scope, t.key)],
);

/* One row per command, written inside the command's own transaction. */
export const audit = pgTable(
  'audit',
  {
    id: bigserial('id', { mode: 'number' }).primaryKey(),
    actorPersonId: integer('actor_person_id').references(() => person.id, {
      onDelete: 'set null',
    }),
    command: text('command').notNull(),
    targetKind: text('target_kind'),
    targetId: integer('target_id'),
    payload: jsonb('payload'),
    at: timestamp('at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    index('audit_at_idx').on(t.at),
    index('audit_actor_idx').on(t.actorPersonId),
    index('audit_target_idx').on(t.targetKind, t.targetId),
  ],
);
