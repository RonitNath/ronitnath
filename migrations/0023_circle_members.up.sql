CREATE TABLE circle_members (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    circle_id INTEGER NOT NULL REFERENCES circles(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(circle_id, person_id)
);
CREATE INDEX idx_circle_members_person ON circle_members(person_id);
