-- Exemplar: every domain table gets account_id. The old unscoped demo
-- data doesn't map to any account, so it's dropped rather than backfilled
-- — a fresh stage_2 fork's guestbook starts empty per account, populated
-- by signing up and signing the (now private, account-owned) guestbook.
DROP TABLE guestbook_entries;

CREATE TABLE guestbook_entries (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    author TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_guestbook_entries_account ON guestbook_entries(account_id);
