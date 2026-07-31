DROP TABLE guestbook_entries;

CREATE TABLE guestbook_entries (
    id INTEGER PRIMARY KEY NOT NULL,
    author TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO guestbook_entries (author, message)
VALUES ('stage_1', 'Welcome! Leave a note below.');
