CREATE TABLE photos (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    uploaded_by_identity_id INTEGER REFERENCES identities(id),
    uploaded_by_person_id INTEGER REFERENCES people(id),
    storage_key TEXT NOT NULL,
    original_filename TEXT NOT NULL DEFAULT '',
    mime_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    caption TEXT NOT NULL DEFAULT '',
    taken_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT
);
CREATE INDEX idx_photos_event ON photos(event_id, deleted_at);
CREATE INDEX idx_photos_storage_key ON photos(storage_key);
