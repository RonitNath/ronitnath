-- Migration 4 — the index "invitations I minted" needs.
--
-- `relation.granted_by` records the identity that wrote a row. Nothing read it
-- until the member tier grew a page of the invitations a person has minted,
-- and that page's statement filters on it: `WHERE rel.subject_kind = 'link'
-- AND rel.granted_by = $1`. With no index on the column that is a scan of
-- every relation row in the deployment — a table that grows with every share,
-- every membership and every invitation ever made — to find the handful one
-- person minted.
--
-- The column order is the query's: seek by minter, and `subject_kind` narrows
-- within that seek rather than after it. `subject_id` rides along so the join
-- to `link` is fed from the index without visiting the table.
--
-- Migrations 1–3 are frozen: hiqlite hashes them, so a `CREATE INDEX` appended
-- to one would invalidate every already-migrated node.

CREATE INDEX relation_granted_by_idx
    ON relation (granted_by, subject_kind, subject_id);
