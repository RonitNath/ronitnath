-- Migration 1 — the kernel schema (docs/kernel/index.html §Schema).
--
-- Conventions that hold for every table here:
--
--   * `id INTEGER PRIMARY KEY` is the rowid and never leaves the process. The
--     public id is derived from it (`kernel::ids`), so there is no `public_id`
--     column: a ciphertext column would be a second source of truth that could
--     disagree with the row it names.
--   * every instant is `INTEGER` unix **seconds**.
--   * every status vocabulary is a CHECK constraint, so a status no command
--     produces cannot be written by any route, including a future one.
--   * anything added after this file was first applied goes in a new migration
--     file (2_relations.sql, 3_merge.sql); this file is frozen.
--
-- Every table the model has is created here — membership, resource, relation,
-- person_link, person_alias and match_candidate included — because the schema
-- is one artefact and hiqlite hashes migration files: appending a column here
-- would invalidate every already-migrated node. The later files hold the
-- columns, indexes and triggers that arrived after the freeze.

-- Foreign keys are on: hiqlite sets the pragma on every state-machine
-- connection, and so does the in-process engine. A PRAGMA in this file would
-- be a no-op, since a migration runs inside a transaction.

-- ---------------------------------------------------------------- party ----
-- Anything that can own, be granted, or belong.
CREATE TABLE party
(
    id           INTEGER PRIMARY KEY,
    kind         TEXT    NOT NULL CHECK (kind IN ('person', 'organization', 'group', 'service')),
    display_name TEXT    NOT NULL,
    status       TEXT    NOT NULL CHECK (status IN ('active', 'disabled', 'merged')),
    created_at   INTEGER NOT NULL
);

CREATE INDEX party_kind_status_idx ON party (kind, status);

-- ------------------------------------------------------------- identity ----
-- A registration, not a human: source-scoped, holds the factors proven for
-- that source, `person_id` NULL until a merge resolves it.
CREATE TABLE identity
(
    id         INTEGER PRIMARY KEY,
    source     TEXT    NOT NULL,
    person_id  INTEGER REFERENCES party (id),
    home_zone  TEXT    NOT NULL,
    status     TEXT    NOT NULL CHECK (status IN ('active', 'disabled')),
    created_at INTEGER NOT NULL
);

CREATE INDEX identity_person_idx ON identity (person_id) WHERE person_id IS NOT NULL;
CREATE INDEX identity_source_idx ON identity (source, status);

-- --------------------------------------------------------------- factor ----
-- What an identity has proven. `passkey` and `oidc` are admitted by the CHECK
-- and produced by no command in this cut: the column is the schema half of a
-- feature whose code lands later, and admitting them now means that code adds
-- no migration.
CREATE TABLE factor
(
    id          INTEGER PRIMARY KEY,
    identity_id INTEGER NOT NULL REFERENCES identity (id),
    kind        TEXT    NOT NULL CHECK (kind IN ('email', 'password', 'passkey', 'oidc')),
    -- The address for `email`, the argon2id PHC string for `password`, the
    -- credential for the other two. Never returned by any query.
    value       TEXT    NOT NULL,
    verified_at INTEGER,
    created_at  INTEGER NOT NULL
);

-- One registration per address. Partial, because two identities may of course
-- share a password hash and `value` is only an identifier for `email`. This is
-- also the sign-in lookup index.
CREATE UNIQUE INDEX factor_email_unique_idx ON factor (value) WHERE kind = 'email';
CREATE INDEX factor_identity_idx ON factor (identity_id, kind);

-- ---------------------------------------------------------- person_link ----
-- Append-only: how an identity came to be attached to a person. Merge writes
-- rows here and never deletes them, which is what makes Split possible.
CREATE TABLE person_link
(
    id          INTEGER PRIMARY KEY,
    identity_id INTEGER NOT NULL REFERENCES identity (id),
    person_id   INTEGER NOT NULL REFERENCES party (id),
    method      TEXT    NOT NULL CHECK (method IN ('self', 'factor', 'operator', 'import')),
    evidence    TEXT,
    asserted_by INTEGER REFERENCES identity (id),
    at          INTEGER NOT NULL
);

CREATE INDEX person_link_identity_idx ON person_link (identity_id, at);
CREATE INDEX person_link_person_idx ON person_link (person_id);

-- --------------------------------------------------------- person_alias ----
-- The absorbed person's id keeps resolving after a merge, so old URLs live.
CREATE TABLE person_alias
(
    old_person_id INTEGER PRIMARY KEY REFERENCES party (id),
    person_id     INTEGER NOT NULL REFERENCES party (id),
    at            INTEGER NOT NULL
);

CREATE INDEX person_alias_person_idx ON person_alias (person_id);

-- ------------------------------------------------------ match_candidate ----
-- Signals propose; proof disposes. `identity_a < identity_b` is enforced so a
-- pair has exactly one row per signal however it was observed.
CREATE TABLE match_candidate
(
    id         INTEGER PRIMARY KEY,
    identity_a INTEGER NOT NULL REFERENCES identity (id),
    identity_b INTEGER NOT NULL REFERENCES identity (id),
    signal     TEXT    NOT NULL CHECK (signal IN ('verified_phone', 'verified_email',
                                                  'oidc_subject', 'claimed_link', 'name_and_group')),
    score      REAL    NOT NULL,
    status     TEXT    NOT NULL CHECK (status IN ('proposed', 'confirmed', 'rejected')),
    created_at INTEGER NOT NULL,
    CHECK (identity_a < identity_b)
);

CREATE UNIQUE INDEX match_candidate_pair_idx ON match_candidate (identity_a, identity_b, signal);
CREATE INDEX match_candidate_queue_idx ON match_candidate (score) WHERE status = 'proposed';

-- ----------------------------------------------------------- membership ----
-- Membership of a group or an organization. `group_id` names either: both are
-- parties, and the role vocabulary is the same one.
CREATE TABLE membership
(
    group_id INTEGER NOT NULL REFERENCES party (id),
    party_id INTEGER NOT NULL REFERENCES party (id),
    role     TEXT    NOT NULL CHECK (role IN ('member', 'admin', 'owner')),
    at       INTEGER NOT NULL,
    PRIMARY KEY (group_id, party_id)
);

CREATE INDEX membership_party_idx ON membership (party_id, role);

-- -------------------------------------------------------------- session ----
-- A session row *is* the session: there is no status, because ending one is
-- deleting it. It binds an identity, never a person, so a merge leaves every
-- session working. `acting_as` is the party the principal is speaking as.
CREATE TABLE session
(
    id           INTEGER PRIMARY KEY,
    identity_id  INTEGER NOT NULL REFERENCES identity (id),
    acting_as    INTEGER NOT NULL REFERENCES party (id),
    -- SHA-256 of a 256-bit random token. The token itself is returned once and
    -- is never stored, so a database read cannot mint a session.
    token_hash   BLOB    NOT NULL UNIQUE,
    expires_at   INTEGER NOT NULL,
    created_at   INTEGER NOT NULL,
    -- An observation, written on the async lane at most once per 5 min per
    -- session. Nothing authoritative reads it.
    last_seen_at INTEGER NOT NULL
);

CREATE INDEX session_identity_idx ON session (identity_id);
CREATE INDEX session_expires_idx ON session (expires_at);

-- ----------------------------------------------------------------- link ----
-- A bearer secret with an expiry. An invitation carries its grant as a relation
-- row (`group:X #member @link:T`), which is why there is no role column here.
-- `verifies_factor_id` is the other purpose a link is minted for — proving an
-- email address — and it stays a column because there is no relation to
-- express "this token verifies that factor".
CREATE TABLE link
(
    id                    INTEGER PRIMARY KEY,
    token_hash            BLOB    NOT NULL UNIQUE,
    expires_at            INTEGER NOT NULL,
    claimed_by_identity_id INTEGER REFERENCES identity (id),
    claimed_at            INTEGER,
    verifies_factor_id    INTEGER REFERENCES factor (id),
    created_at            INTEGER NOT NULL
);

CREATE INDEX link_expires_idx ON link (expires_at);

-- ------------------------------------------------------------- resource ----
-- Every ownable thing has a row here; kind tables hang off it 1:1.
CREATE TABLE resource
(
    id             INTEGER PRIMARY KEY,
    kind           TEXT    NOT NULL,
    owner_party_id INTEGER NOT NULL REFERENCES party (id),
    home_zone      TEXT    NOT NULL,
    status         TEXT    NOT NULL CHECK (status IN ('draft', 'published', 'deleted')),
    created_at     INTEGER NOT NULL
);

CREATE INDEX resource_owner_idx ON resource (owner_party_id, kind, status);

-- "Groups never own" is an invariant of the model, so it is a constraint
-- rather than a rule some command remembers to apply. A CHECK cannot see
-- another table; a trigger can.
CREATE TRIGGER resource_owner_is_never_a_group_insert
    BEFORE INSERT
    ON resource
    FOR EACH ROW
    WHEN (SELECT kind FROM party WHERE id = NEW.owner_party_id) IN ('group', 'service')
BEGIN
    SELECT RAISE(ABORT, 'a group never owns a resource');
END;

CREATE TRIGGER resource_owner_is_never_a_group_update
    BEFORE UPDATE OF owner_party_id
    ON resource
    FOR EACH ROW
    WHEN (SELECT kind FROM party WHERE id = NEW.owner_party_id) IN ('group', 'service')
BEGIN
    SELECT RAISE(ABORT, 'a group never owns a resource');
END;

-- ------------------------------------------------------------- relation ----
-- `object #relation @subject` answers every "may X do Y to Z". Nesting
-- (viewer<commenter<editor, member<admin<owner) is applied in code; a row
-- names exactly one relation.
--
-- `public` and `authenticated` are subjects with no row to point at, so they
-- carry `subject_id = 0`: a NULL would make the UNIQUE index stop deduplicating
-- them, since SQLite treats NULLs as distinct.
CREATE TABLE relation
(
    id           INTEGER PRIMARY KEY,
    object_kind  TEXT    NOT NULL,
    object_id    INTEGER NOT NULL,
    relation     TEXT    NOT NULL,
    subject_kind TEXT    NOT NULL CHECK (subject_kind IN ('person', 'organization', 'group',
                                                          'service', 'identity', 'link',
                                                          'public', 'authenticated')),
    subject_id   INTEGER NOT NULL,
    granted_by   INTEGER REFERENCES identity (id),
    at           INTEGER NOT NULL,
    CHECK ((subject_kind IN ('public', 'authenticated')) = (subject_id = 0))
);

CREATE UNIQUE INDEX relation_unique_idx
    ON relation (object_kind, object_id, relation, subject_kind, subject_id);
CREATE INDEX relation_object_idx ON relation (object_kind, object_id);
CREATE INDEX relation_subject_idx ON relation (subject_kind, subject_id);

-- ---------------------------------------------------------------- audit ----
-- One row per command, written inside the command's own transaction.
--
-- `audit.id` *is* the change-feed offset: a consumer resumes by asking for
-- rows after the last id it applied. A separate `event_offset` column would be
-- a second counter that could disagree with the row order it describes.
--
-- `key` is the caller's idempotency key. It is UNIQUE and NOT NULL, so a
-- replay collides at the database rather than at a check some command might
-- skip; `request_digest` is what tells a replay of the same body (return the
-- original result) from a reuse of the key with a different body (conflict).
CREATE TABLE audit
(
    id                INTEGER PRIMARY KEY,
    key               TEXT    NOT NULL UNIQUE,
    command           TEXT    NOT NULL,
    actor_identity_id INTEGER REFERENCES identity (id),
    acting_as         INTEGER REFERENCES party (id),
    at                INTEGER NOT NULL,
    request_digest    TEXT    NOT NULL,
    -- The typed event, as JSON. This is what `feed::read` returns.
    payload           TEXT    NOT NULL
);

CREATE INDEX audit_actor_idx ON audit (actor_identity_id, id);
