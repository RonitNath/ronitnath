/* The kernel model, simplified for Postgres (docs/plan.md §Model). Internal
 * ids are integers and never leave the server; the public id codec in
 * src/lib/ids.ts is the only thing that turns one into a string. Feature
 * modules take their slice of this file as they land. */

import { sql } from 'drizzle-orm';
import {
  type AnyPgColumn,
  bigint,
  bigserial,
  boolean,
  check,
  customType,
  index,
  integer,
  jsonb,
  pgEnum,
  pgTable,
  primaryKey,
  serial,
  smallint,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from 'drizzle-orm/pg-core';
import type { RealtimeSubscription } from '@isoastra/fleet-events/presence';

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
export const matchSignal = pgEnum('match_signal', [
  'verified_email',
  'claimed_link',
  'operator',
]);
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
    /* The one join between the kernel model and the identity core. A guest is
     * a person with no user: events, invites, rsvps and photos all reference
     * `person`, never `auth.user`, so somebody who was only ever named by a
     * host keeps working and becomes a member the day a user row is linked
     * here (fleet-conventions §2). No foreign key: `auth.user` is better-auth's
     * table in its own schema, and a cross-schema constraint would make
     * deleting a user a decision this table gets to veto. */
    userId: text('user_id'),
    createdAt: now(),
  },
  (t) => [
    index('person_merged_into_idx').on(t.mergedInto),
    uniqueIndex('person_user_id_key').on(t.userId),
  ],
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
    /* Impersonation (R6). A session whose principal is the target person and
     * whose actor, in every audit row it writes, is the operator who started
     * it. Null on every ordinary session, which is what makes the bar and the
     * double-named audit row impossible to forget. */
    actingOperatorId: integer('acting_operator_id').references((): AnyPgColumn => person.id, {
      onDelete: 'set null',
    }),
    /* When a password or a fresh OIDC round trip was last presented on this
     * session. The commands that destroy or impersonate ask for one inside
     * the last ten minutes (src/features/platform/reauth.ts). */
    reauthenticatedAt: timestamp('reauthenticated_at', { withTimezone: true }),
    lastSeenAt: timestamp('last_seen_at', { withTimezone: true }).notNull().defaultNow(),
    expiresAt: timestamp('expires_at', { withTimezone: true }).notNull(),
    revokedAt: timestamp('revoked_at', { withTimezone: true }),
    createdAt: now(),
  },
  (t) => [
    uniqueIndex('session_token_hash_key').on(t.tokenHash),
    index('session_person_idx').on(t.personId),
    index('session_acting_operator_idx').on(t.actingOperatorId),
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

/* A group is a bag of subjects with a name, owned by whoever made it — a
 * person or an organization. It is not an organization's private furniture:
 * a member's own circles are groups too, which is what the guest ordering on
 * an event page reads. Nesting is a group inside a group, and the only thing
 * forbidden is a cycle (src/lib/authority.ts). */
export const group = pgTable(
  'group',
  {
    id: serial('id').primaryKey(),
    ownerPartyId: integer('owner_party_id')
      .notNull()
      .references(() => party.id, { onDelete: 'cascade' }),
    name: text('name').notNull(),
    createdAt: now(),
  },
  (t) => [index('group_owner_idx').on(t.ownerPartyId)],
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
    /* Markdown-lite, as it was typed; the HTML is rendered on the way out by
     * the events `markup` module, so there is one place markup can be born. */
    body: text('body').notNull().default(''),
    draftVersion: integer('draft_version').notNull().default(0),
    titleVersion: integer('title_version').notNull().default(0),
    bodyVersion: integer('body_version').notNull().default(0),
    /* Derived from the title once, at creation, and then it belongs to the
     * URL: a rewritten title does not break a link somebody already pasted. */
    slug: text('slug'),
    publishedAt: timestamp('published_at', { withTimezone: true }),
    publishedTitle: text('published_title'),
    publishedBody: text('published_body'),
    publishedVersion: integer('published_version'),
    updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
    createdAt: now(),
  },
  (t) => [
    index('document_owner_idx').on(t.ownerPartyId),
    uniqueIndex('document_slug_key').on(t.slug),
  ],
);

export const documentRevision = pgTable(
  'document_revision',
  {
    id: bigserial('id', { mode: 'number' }).primaryKey(),
    documentId: integer('document_id')
      .notNull()
      .references(() => document.id, { onDelete: 'cascade' }),
    version: integer('version').notNull(),
    mutationId: text('mutation_id').notNull(),
    kind: text('kind').notNull(),
    fields: jsonb('fields').$type<string[]>().notNull().default([]),
    title: text('title').notNull(),
    body: text('body').notNull(),
    previous: jsonb('previous'),
    actorPersonId: integer('actor_person_id').references(() => person.id, {
      onDelete: 'set null',
    }),
    createdAt: now(),
  },
  (t) => [
    uniqueIndex('document_revision_mutation_key').on(t.documentId, t.mutationId),
    index('document_revision_history_idx').on(t.documentId, t.version),
  ],
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
    /* The two halves of a place. `location` is the name a guest may read
     * before answering ("Ronit's apartment"); the address is the thing that
     * only a yes buys. */
    location: text('location'),
    address: text('address'),
    /* An instant is an instant, but "Sunday afternoon" is a wall clock in a
     * particular city: the host's zone is what the guest page falls back to
     * and what the calendar entry is written in. */
    timezone: text('timezone').notNull().default('America/Los_Angeles'),
    /* Markdown-lite, as the host typed it. The HTML a guest reads is rendered
     * from this on the way out and never stored, so there is exactly one
     * place where markup can be born (src/features/events/markup.ts). */
    body: text('body').notNull().default(''),
    /* Null is no limit. A yes past the limit is still a yes — it is `full`,
     * a word on the page, not an error (src/features/events/capacity.ts). */
    capacity: integer('capacity'),
    /* One of the token names in src/features/events/palette.ts, or a poster
     * the host points at. Neither is required and neither is free-form CSS. */
    colour: text('colour'),
    posterUrl: text('poster_url'),
    /* Whether a guest who has answered yes sees full names or first names. */
    revealGuests: boolean('reveal_guests').notNull().default(false),
    /* RFC 5545 SEQUENCE. Bumped by every host edit that a calendar would
     * want to hear about, so an entry already in somebody's calendar is
     * replaced rather than duplicated. */
    sequence: integer('sequence').notNull().default(0),
    publishedAt: timestamp('published_at', { withTimezone: true }),
    updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
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
    /* The personal link minted for this guest at publication. The row keeps
     * the link's id, never its token: the URL is shown once, at mint time,
     * and the host mints another one if they lose it. */
    linkId: integer('link_id').references(() => link.id, { onDelete: 'set null' }),
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

/* Raw bytes, as a column. There is no object store in this deployment — no
 * bucket, no CDN, no signed URL — and adding one to carry a handful of party
 * photographs would be a second system to provision, secure and back up for a
 * feature that the database this app already has can hold. A photograph is a
 * few megabytes; `bytea` puts it in TOAST storage out of line with the row, so
 * a query that does not name this column does not read it.
 *
 * The boundary this buys is worth more than the bytes it costs: a picture in
 * the table is a picture inside the same transaction, the same backup and the
 * same delete-cascade as the event it belongs to, and it cannot be reached by
 * guessing a URL on a public bucket. */
const bytea = customType<{ data: Buffer; driverData: Buffer }>({
  dataType: () => 'bytea',
});

/* A picture a guest added after the evening started.
 *
 * `hidden_at` rather than a delete, which is the donor's ruling and the right
 * one: taking a picture off the gallery is the host's everyday act, it has to
 * be reversible, and the row is also the only thing that says the picture was
 * ever there. A hidden picture is absent from the gallery's HTML rather than
 * styled out of it — the host's act has to mean the bytes stop being named.
 *
 * `person_id` is who added it and is nullable only because a person may be
 * merged or removed later; the picture stays with the event either way. */
export const photo = pgTable(
  'photo',
  {
    id: serial('id').primaryKey(),
    eventId: integer('event_id')
      .notNull()
      .references(() => event.id, { onDelete: 'cascade' }),
    personId: integer('person_id').references(() => person.id, { onDelete: 'set null' }),
    /* One of the three the upload path accepts, and the one the bytes were
     * sniffed as rather than the one the browser claimed. */
    mimeType: text('mime_type').notNull(),
    bytes: bytea('bytes').notNull(),
    byteSize: integer('byte_size').notNull(),
    /* Read out of the file's own header on the way in, so the grid can
     * reserve the right box before an image loads. */
    width: integer('width').notNull(),
    height: integer('height').notNull(),
    hiddenAt: timestamp('hidden_at', { withTimezone: true }),
    hiddenBy: integer('hidden_by').references(() => person.id, { onDelete: 'set null' }),
    createdAt: now(),
  },
  (t) => [index('photo_event_idx').on(t.eventId), index('photo_person_idx').on(t.personId)],
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
    /* Who asked the question. Null for the ones the model noticed by itself;
     * the operator's id when they picked two parties by hand. */
    proposedBy: integer('proposed_by').references(() => person.id, { onDelete: 'set null' }),
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
    windowStartedAt: timestamp('window_started_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
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

/* The sky's star detail, one row per star the panel can be opened on.
 *
 * This is not part of the kernel model: it is a read-only catalogue, built
 * offline by `tools/starcat/build_detail.py` and loaded whole by
 * `pnpm db:load-sky`. It has no foreign keys, no audit and no commands — a
 * detail lookup is a read, and reads write nothing (docs/plan.md).
 *
 * `id` is the *dataset's* form, `g<gaia source_id>` or `h<hip>`, which is what
 * the CSV carries and what a three-million-row COPY must not have to rewrite.
 * The URL a browser asks with is `gaia-<id>` / `hip-<n>` (`catalog.ts`
 * `starKey()`); the route translates between the two and nothing else does.
 * The payload is the builder's JSON verbatim, normalised on the way out.
 *
 * The index is on the Hipparcos number *inside* the payload, because a star
 * has two names here and only one of them is the key. The bright catalogue
 * calls a first-magnitude star by its HIP number, but the detail build wrote
 * it under its Gaia source id wherever the two matched — Alioth is
 * `g1576683529448755328` with `hip: "62956"` in the payload, and only the 66
 * Hipparcos stars Gaia has no row for are keyed `h<n>`. Without this index the
 * fallback lookup is a sequential scan of three million rows. */
export const skyStarDetail = pgTable(
  'sky_star_detail',
  {
    id: text('id').primaryKey(),
    payload: jsonb('payload').notNull(),
    builtAt: timestamp('built_at', { withTimezone: true }).notNull().defaultNow(),
  },
  () => [index('sky_star_detail_hip_idx').on(sql`((payload ->> 'hip'))`)],
);

/* ── The spine ───────────────────────────────────────────────────────────────
 *
 * `domain_event` is the one append-only log every mutating transaction writes
 * to (fleet-conventions §7). Realtime, audit, notifications and metering all
 * read it, so a command that forgets to `emit` is invisible to all four.
 *
 * `seq` is gapless *per org*, assigned by a BEFORE INSERT trigger that takes a
 * row lock on the org's cursor — that is what lets a reconnecting browser say
 * `since=<seq>` and be certain it missed nothing. `org_id` here is a public
 * id, not an integer: a person's or an organization's, because on this site a
 * user's own stream is as much a stream as an organization's.
 *
 * Append-only is enforced in the database, not in the application: the UPDATE
 * and DELETE triggers raise. */
export const domainEvent = pgTable(
  'domain_event',
  {
    id: bigserial('id', { mode: 'number' }).primaryKey(),
    orgId: text('org_id').notNull(),
    seq: bigint('seq', { mode: 'number' }).notNull(),
    resourceKind: text('resource_kind').notNull(),
    resourceId: text('resource_id').notNull(),
    kind: text('kind').notNull(),
    actorId: text('actor_id'),
    subjectId: text('subject_id'),
    actingOperatorId: text('acting_operator_id'),
    correlationId: text('correlation_id').notNull(),
    published: boolean('published').notNull().default(true),
    payload: jsonb('payload'),
    at: timestamp('at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    uniqueIndex('domain_event_org_seq').on(t.orgId, t.seq),
    index('domain_event_resource_idx').on(t.resourceKind, t.resourceId),
    index('domain_event_at_idx').on(t.at),
  ],
);

export const domainEventCursor = pgTable('domain_event_cursor', {
  orgId: text('org_id').primaryKey(),
  seq: bigint('seq', { mode: 'number' }).notNull().default(0),
});

/* ── Configured records ──────────────────────────────────────────────────────
 *
 * Anything with a publish boundary: a draft row and a published row under the
 * same key, and a version history beside them. Watchers refresh on `published`
 * only; a save that carries a stale version is a 409 with the diff, never a
 * merge (fleet-conventions §7, contracts.md). */
export const configured = pgTable(
  'configured',
  {
    orgId: text('org_id').notNull(),
    key: text('key').notNull(),
    state: text('state').notNull(),
    version: integer('version').notNull().default(0),
    body: jsonb('body').notNull(),
    updatedBy: text('updated_by'),
    updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [
    primaryKey({ columns: [t.orgId, t.key, t.state] }),
    check('configured_state', sql`${t.state} IN ('draft','published')`),
  ],
);

export const configuredVersion = pgTable(
  'configured_version',
  {
    orgId: text('org_id').notNull(),
    key: text('key').notNull(),
    version: integer('version').notNull(),
    body: jsonb('body').notNull(),
    publishedBy: text('published_by'),
    at: timestamp('at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.orgId, t.key, t.version] })],
);

/* Browser presence is shared database state. Every process may hold sockets,
 * but the rows below are what decides who is owed an update and what the
 * operator console inspects. */
export const realtimeView = pgTable(
  'realtime_view',
  {
    id: uuid('id').primaryKey(),
    visitorId: uuid('visitor_id').notNull(),
    tabId: uuid('tab_id').notNull(),
    sessionId: text('session_id'),
    personId: text('person_id'),
    path: text('path').notNull(),
    title: text('title').notNull().default(''),
    visible: boolean('visible').notNull().default(true),
    visibleSections: jsonb('visible_sections').$type<string[]>().notNull().default([]),
    visibleRows: jsonb('visible_rows').$type<string[]>().notNull().default([]),
    interactedAt: timestamp('interacted_at', { withTimezone: true }),
    connectedAt: timestamp('connected_at', { withTimezone: true }).notNull().defaultNow(),
    lastHeartbeatAt: timestamp('last_heartbeat_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
    leaseExpiresAt: timestamp('lease_expires_at', { withTimezone: true }).notNull(),
    disconnectedAt: timestamp('disconnected_at', { withTimezone: true }),
  },
  (t) => [
    uniqueIndex('realtime_view_visitor_tab_key').on(t.visitorId, t.tabId),
    index('realtime_view_lease_idx').on(t.leaseExpiresAt),
    index('realtime_view_person_idx').on(t.personId, t.lastHeartbeatAt),
  ],
);

export const realtimeSubscription = pgTable(
  'realtime_subscription',
  {
    viewId: uuid('view_id')
      .notNull()
      .references(() => realtimeView.id, { onDelete: 'cascade' }),
    subscriptionKey: text('subscription_key').notNull(),
    descriptor: jsonb('descriptor').$type<RealtimeSubscription>().notNull(),
    authorizedAt: timestamp('authorized_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.viewId, t.subscriptionKey] })],
);

export const realtimeDelivery = pgTable(
  'realtime_delivery',
  {
    id: uuid('id').primaryKey(),
    viewId: uuid('view_id')
      .notNull()
      .references(() => realtimeView.id, { onDelete: 'cascade' }),
    eventOrgId: text('event_org_id').notNull(),
    eventSeq: bigint('event_seq', { mode: 'number' }).notNull(),
    revision: text('revision').notNull(),
    reason: text('reason').notNull(),
    selectedAt: timestamp('selected_at', { withTimezone: true }).notNull().defaultNow(),
    sentAt: timestamp('sent_at', { withTimezone: true }),
    receivedAt: timestamp('received_at', { withTimezone: true }),
    appliedAt: timestamp('applied_at', { withTimezone: true }),
  },
  (t) => [
    uniqueIndex('realtime_delivery_view_event_key').on(t.viewId, t.eventOrgId, t.eventSeq),
    index('realtime_delivery_view_idx').on(t.viewId, t.selectedAt),
  ],
);
