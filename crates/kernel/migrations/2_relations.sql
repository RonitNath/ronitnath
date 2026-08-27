-- Migration 2 — what the relation store needs that migration 1 did not carry.
--
-- `party`, `membership`, `resource` and `relation` are created by
-- `1_kernel.sql` and that file is frozen: hiqlite hashes migrations, so a
-- column appended there would invalidate every already-migrated node. Only new
-- objects live here.
--
-- Three things are added:
--
--   * `relation.subject_key` — the subject as one text value, so the hot
--     `check()` query can seek a set of subjects through a single index
--     instead of one `OR` branch per subject kind. It is GENERATED, so it
--     cannot disagree with the columns it is derived from, and VIRTUAL,
--     because that is the only kind of generated column `ALTER TABLE` admits.
--   * the relation vocabulary, as a trigger. `relation.relation` has no CHECK
--     constraint in migration 1 and SQLite cannot add one to an existing
--     table; a trigger says the same thing, and `relation::vocabulary`'s test
--     reads this list back out of `sqlite_master` and compares it with the
--     Rust enum, so the two cannot drift.
--   * the kind tables that hang off `resource` 1:1 — `party_resource` for the
--     two party-backed kinds (organization, group) and `document`.
--
-- ------------------------------------------------------- ownership model ----
-- An organization is two rows, and so is a group:
--
--   party(kind='organization'|'group')   the thing that is granted and belongs
--   resource(kind='organization'|'group', owner_party_id=…)   the thing owned
--   party_resource(resource_id, party_id)                     the 1:1 join
--
-- An organization's *resource* row is owned by the founding person and moves
-- only through `Transfer`; its *party* row is what memberships and relations
-- point at. A group is the same shape with the owner being the person or the
-- organization it was created under. Groups never appear as an owner
-- themselves — migration 1's trigger makes that unrepresentable rather than
-- remembered.
--
-- An organization is also its own root group: "the members of org O" is
-- `membership` rows whose `group_id` is O's own party id, which is why one
-- membership table and one role vocabulary cover both.

-- ------------------------------------------------------------- relation ----
ALTER TABLE relation
    ADD COLUMN subject_key TEXT GENERATED ALWAYS AS (subject_kind || ':' || subject_id) VIRTUAL;

-- The `check()` index: seek by subject, then by object, and read the relation
-- out of the index itself. Every column the hot query touches is in here, so
-- it never visits the table.
CREATE INDEX relation_subject_key_idx
    ON relation (subject_key, object_kind, object_id, relation);

-- The relation vocabulary. Nesting (viewer<commenter<editor<owner,
-- member<admin<owner) is applied in code; a row names exactly one of these.
CREATE TRIGGER relation_vocabulary_insert
    BEFORE INSERT
    ON relation
    FOR EACH ROW
    WHEN NEW.relation NOT IN ('viewer', 'commenter', 'editor', 'owner',
                              'member', 'admin', 'contact', 'operator')
BEGIN
    SELECT RAISE(ABORT, 'not a relation this deployment knows');
END;

-- ------------------------------------------------------- party_resource ----
-- The 1:1 kind table for the two party-backed resource kinds. `party_id` is
-- UNIQUE, so a party is owned through exactly one resource row.
CREATE TABLE party_resource
(
    resource_id INTEGER PRIMARY KEY REFERENCES resource (id),
    party_id    INTEGER NOT NULL UNIQUE REFERENCES party (id)
);

-- ------------------------------------------------------------- document ----
-- The first product kind. `draft_rev` is the version tag an edit carries: a
-- client sends the revision it read, the update guards on it, and two people
-- editing the same paragraph is a decline rather than a silent overwrite.
-- `published_rev` is NULL until `PublishDocument` copies `draft_rev` into it.
CREATE TABLE document
(
    resource_id   INTEGER PRIMARY KEY REFERENCES resource (id),
    title         TEXT    NOT NULL,
    body          TEXT    NOT NULL,
    draft_rev     INTEGER NOT NULL,
    published_rev INTEGER
);

-- ------------------------------------------------------------- resource ----
-- `list_visible` pages by (created_at, id) within one kind, so the order it
-- wants is the order this index is already in and there is no sort.
CREATE INDEX resource_kind_created_idx
    ON resource (kind, status, created_at DESC, id DESC);
