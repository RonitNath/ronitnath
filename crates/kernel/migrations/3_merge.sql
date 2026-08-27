-- Migration 3 — what merge needs beyond migration 1.
--
-- `person_link`, `person_alias` and `match_candidate` are created in
-- `1_kernel.sql`, because the schema is one artefact and hiqlite hashes
-- migration files: appending a column to an applied file invalidates every
-- migrated node. This file therefore holds only what merge adds to them —
-- one column, two triggers and three indexes.

-- ---------------------------------------------------------- person_link ----
-- Where the identity was before this link, so `Split` is a real inverse
-- rather than a detach.
--
-- Merge unions the absorbed person's memberships and relations onto the
-- survivor; it does not delete the absorbed person's own rows, which stay as
-- the record of what that person held. `Split` reads this column to find that
-- record and copies it onto the fresh person, so the set a split-off identity
-- lands with is the set it arrived with. NULL for a registration that has
-- never been linked to anybody else.
--
-- Nullable with no default, which is also what SQLite requires of a column
-- added with a REFERENCES clause while foreign keys are on.
ALTER TABLE person_link
    ADD COLUMN from_person_id INTEGER REFERENCES party (id);

-- Append-only, enforced rather than remembered.
--
-- "`person_link` is append-only" is the sentence that makes merge reversible:
-- Split reconstructs from these rows, and a row that could be edited or
-- deleted would take that reconstruction with it. A comment saying so is a
-- request; a trigger is the schema refusing.
CREATE TRIGGER person_link_is_append_only_update
    BEFORE UPDATE
    ON person_link
    FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'person_link is append-only');
END;

CREATE TRIGGER person_link_is_append_only_delete
    BEFORE DELETE
    ON person_link
    FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'person_link is append-only');
END;

-- --------------------------------------------------------------- signals ----
-- The scan asks "which other identity has proven this same value", once per
-- verified factor of the identity that just changed. Without this index that
-- question is a scan of every factor in the deployment on every registration.
--
-- Partial on `verified_at IS NOT NULL`, because an unproven address is not a
-- signal about anybody: two people may both *claim* an address and only one
-- can prove it.
CREATE INDEX factor_verified_value_idx ON factor (value, kind, identity_id)
    WHERE verified_at IS NOT NULL;

-- ------------------------------------------------------------- candidate ----
-- Migration 1's UNIQUE (identity_a, identity_b, signal) already answers
-- "candidates of identity_a"; nothing answers "candidates of identity_b", and
-- both sides of a pair are asked about — the queue page of a person, and the
-- scan's own dedupe, name whichever identity they hold.
CREATE INDEX match_candidate_b_idx ON match_candidate (identity_b, status);

-- The person-level queue: "what is proposed about me" joins candidates to the
-- identities of a person, so the identity_a side wants its own index rather
-- than borrowing the UNIQUE index's leading column with a status filter on
-- top of it.
CREATE INDEX match_candidate_a_idx ON match_candidate (identity_a, status);
