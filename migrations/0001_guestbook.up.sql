-- NOT NULL on the primary key: sqlite's PK is nullable by default (it's a
-- rowid alias), and without this sqlx would infer `id: Option<i64>`.
CREATE TABLE guestbook_entries (
    id INTEGER PRIMARY KEY NOT NULL,
    author TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
