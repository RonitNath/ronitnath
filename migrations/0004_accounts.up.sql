-- account = the unit of legal ownership. All domain data FKs to an
-- account, never directly to an identity, so billing/export/deletion
-- happen at one consistent level.
CREATE TABLE accounts (
    id INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('personal', 'org')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT
);
