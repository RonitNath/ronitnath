-- One row per block of the day. Items carry the same two-tier visibility
-- as events. A non-null segment_key marks the block as individually
-- RSVP-able (board_games / dinner / fireworks / rooftop / sleepover ...)
-- and segment_rsvps rows hang off it.
CREATE TABLE schedule_items (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    time_label TEXT NOT NULL,
    title TEXT NOT NULL,
    detail TEXT NOT NULL DEFAULT '',
    tier TEXT NOT NULL DEFAULT 'public'
        CHECK (tier IN ('public', 'private')),
    segment_key TEXT,
    capacity INTEGER
);

CREATE INDEX idx_schedule_items_event ON schedule_items(event_id);
