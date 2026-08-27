-- Migration 7 — what an operator's authority needs to be accountable.
--
-- Five additions, and every one of them exists because a decision the platform
-- stories describe could not otherwise be *found afterwards*. Nothing here
-- grants anything: the grants are relation rows, which migration 1 already
-- carries, and the commands that write them are in `crates/kernel/src/cmd`.
--
-- Migrations 1–6 are frozen: hiqlite hashes them, so a column appended to one
-- would invalidate every already-migrated node. That is why these are here and
-- not there.

-- ------------------------------------------------------- session.auth_time --
-- When a password was last presented on this session.
--
-- Not when the session was created: `ReAuthenticate` moves this and mints no
-- new row, which is the whole of what a re-authentication window is. Without
-- the column nothing in the deployment could say how long ago somebody proved
-- who they were, so "this command needs a password within the last fifteen
-- minutes" was unimplementable rather than merely unimplemented.
--
-- DEFAULT 0 rather than a backfill: a session minted before this migration has
-- no honest answer, and zero reads as "long ago", which refuses the sensitive
-- commands until its holder re-authenticates. The alternative — defaulting to
-- now — would hand every open session a fresh fifteen minutes for free.
ALTER TABLE session ADD COLUMN auth_time INTEGER NOT NULL DEFAULT 0;

-- ---------------------------------------------------------- impersonation --
-- Who is really behind this session, when it is not its own identity.
--
-- NULL for every ordinary session, and that is the shape the design rests on:
-- `SignInAs` mints a *real* session row for an active identity of the target,
-- so `principal::expand` resolves the target's real subject set and `check()`
-- learns no new case. What these two columns add is the record — the operator
-- the audit row names as actor, and the reason they had to state.
ALTER TABLE session ADD COLUMN impersonated_by_identity_id INTEGER REFERENCES identity (id);
ALTER TABLE session ADD COLUMN impersonation_reason TEXT;

-- The rare-kind index. Impersonated sessions are a handful in a deployment's
-- whole history, so a partial index over them is a few pages and answers the
-- two questions that matter: which sessions an operator is wearing, and — at
-- resolve time — whether the operator behind this one still holds the relation.
CREATE INDEX session_impersonated_idx
    ON session (impersonated_by_identity_id)
    WHERE impersonated_by_identity_id IS NOT NULL;

-- ------------------------------------------------------ link.suspended_at --
-- A link that is asleep because the party that minted it is disabled.
--
-- Finding F3: a disable deleted sessions, tokens and codes and did not touch
-- links, so a disabled person's outstanding invitations kept working. This is
-- not a delete, because `Enable` has to put it back and `revoke_link` already
-- means *gone for good* — a link the operator withdrew and a link that is
-- waiting for its minter to be re-enabled are different facts and a deployment
-- that conflated them would re-open invitations somebody deliberately killed.
ALTER TABLE link ADD COLUMN suspended_at INTEGER;

-- ----------------------------------------------------------- audit_object --
-- Which rows an audit row was about.
--
-- Finding F4: `audit` is keyed on the actor, so an operator's ruling *about*
-- you is not in your list — the one thing story 10 says the owner must be able
-- to see. The object is inside the payload, as JSON, and a JSON extract is not
-- indexable: a query that asked "every ruling about this party" by reading the
-- payload would be a full scan of the deployment's entire history, which is
-- also its change feed and has no retention.
--
-- So the objects come out of the payload and into rows, written by the same
-- transaction as the audit row itself — never by a job afterwards, because a
-- job that fell behind would make the record wrong rather than late.
--
-- The primary key is the query's order, not the writer's: seek by the thing
-- somebody is asking about, then walk its rulings newest-first. WITHOUT ROWID
-- because the whole row *is* the key and a rowid would be a second copy of it.
CREATE TABLE audit_object
(
    -- The row it describes. Not a foreign key by accident: `audit` is
    -- append-only and nothing deletes from it, so this can never dangle.
    audit_id INTEGER NOT NULL REFERENCES audit (id),
    -- What kind of thing: 'party', 'session', 'resource', 'link', 'identity',
    -- 'oidc_client', 'factor'. A vocabulary rather than a table name, because
    -- a person and an organization are both `party` rows and a caller asking
    -- about one is not asking about the other's table.
    kind     TEXT    NOT NULL,
    -- Its rowid, under that kind.
    id       INTEGER NOT NULL,
    PRIMARY KEY (kind, id, audit_id)
) WITHOUT ROWID;

-- The other direction: everything one ruling was about, for a panel rendering
-- a single row.
CREATE INDEX audit_object_audit_idx ON audit_object (audit_id);

-- ---------------------------------------------------- the audit's filters --
-- `audit_actor_idx (actor_identity_id, id)` exists from migration 1 and
-- answers "what did this identity do". The operator's audit screen filters on
-- two more things and neither had an index, so both were a scan of a table
-- that grows with every command the deployment has ever run.
--
-- `acting_as` is the *hat* — the party a command was attributed to, which
-- after `ActAs` and under impersonation is not the actor. `(acting_as, id)`
-- in that order so the filter is a seek and the newest-first walk inside it
-- needs no sort.
CREATE INDEX audit_acting_as_idx ON audit (acting_as, id);

-- And the time range, which is the one filter a reader reaches for without
-- naming anybody at all.
CREATE INDEX audit_at_idx ON audit (at);
