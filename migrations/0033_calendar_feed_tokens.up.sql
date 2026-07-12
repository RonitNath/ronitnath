CREATE TABLE calendar_feed_tokens (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    token_plain TEXT NOT NULL,
    revoked_at TEXT,
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(account_id, person_id)
);
