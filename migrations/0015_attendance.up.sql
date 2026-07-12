-- person × event: the longitudinal edge. Backfilled for past events so
-- "who came to what" accumulates over time.
CREATE TABLE attendance (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'none'
        CHECK (status IN ('none', 'going', 'maybe', 'no', 'attended')),
    party_size INTEGER NOT NULL DEFAULT 1 CHECK (party_size >= 1),
    note TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (event_id, person_id)
);

CREATE INDEX idx_attendance_event ON attendance(event_id);
CREATE INDEX idx_attendance_person ON attendance(person_id);
