-- identity = the acting entity (human, agent, or service). Never
-- hard-deleted: tombstoned via deleted_at so audit_log rows keep their
-- actor even after the identity is gone.
CREATE TABLE identities (
    id INTEGER PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('human', 'agent', 'service')),
    display_name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT
);
