-- The longitudinal person registry: one row per human across all events,
-- so attendance history accumulates event over event.
CREATE TABLE people (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    name TEXT NOT NULL,
    -- Loose grouping used for transit/logistics ("Berkeley crew").
    group_label TEXT NOT NULL DEFAULT '',
    contact TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_people_account ON people(account_id);
