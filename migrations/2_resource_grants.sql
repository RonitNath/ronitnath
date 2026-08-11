-- Per-resource sharing grants (ruling 2026-08-11).
--
-- Accounts are the ownership boundary: a resource belongs to the account it was
-- created under, and owner-account members reach it through membership, never
-- through a row here. This table holds only the *exceptions* — the shares.
--
-- A grant names a resource polymorphically by (resource_kind, resource_public_id).
-- This is a deliberate, documented deviation from "integer id is the only join
-- key": a polymorphic reference can never carry a foreign key regardless of key
-- type, and using the public id means the reference is display-safe and the
-- /manage invariant (integer join keys never reach HTML) holds without a join
-- into a table that may not exist yet. Resolution always starts from a loaded
-- resource row, which knows both of its ids, so nothing ever joins through this
-- column back to an integer.
--
-- `role` is a bundle name expanded in code (viewer → read; commenter → +comment;
-- editor → +write), the same pattern as membership role bundles: one word in the
-- row, the meaning in a match the compiler checks.
--
-- Subjects:
--   identity — shared with a person; follows them across their accounts.
--   account  — shared with everyone in an account at once.
--   link     — reserved for capability links (invites / share links). No rows
--              until the links table exists; the CHECK admits it so the schema
--              does not need another migration to start using it.
CREATE TABLE resource_grants
(
    id                 INTEGER PRIMARY KEY NOT NULL,
    resource_kind      TEXT    NOT NULL,
    resource_public_id TEXT    NOT NULL,
    subject_kind       TEXT    NOT NULL
        CHECK (subject_kind IN ('identity', 'account', 'link')),
    subject_id         INTEGER NOT NULL,
    role               TEXT    NOT NULL
        CHECK (role IN ('viewer', 'commenter', 'editor')),
    -- Who made the share: hand-made grants are exactly the kind that need an
    -- audit trail, same reasoning as membership_capabilities.granted_at.
    granted_by_identity_id INTEGER REFERENCES identities (id),
    granted_at         INTEGER NOT NULL,
    UNIQUE (resource_kind, resource_public_id, subject_kind, subject_id)
);

-- "everything shared with me": both the identity-subject and account-subject
-- probes a session resolution makes.
CREATE INDEX resource_grants_subject_idx
    ON resource_grants (subject_kind, subject_id);
