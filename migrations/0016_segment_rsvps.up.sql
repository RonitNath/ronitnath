-- person × schedule segment: which blocks of a long day each person is in
-- for (dinner yes, fireworks yes, sleepover no ...).
CREATE TABLE segment_rsvps (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    schedule_item_id INTEGER NOT NULL REFERENCES schedule_items(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('in', 'maybe', 'out')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (schedule_item_id, person_id)
);

CREATE INDEX idx_segment_rsvps_item ON segment_rsvps(schedule_item_id);
CREATE INDEX idx_segment_rsvps_person ON segment_rsvps(person_id);
