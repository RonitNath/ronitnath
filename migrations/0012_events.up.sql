-- Events. Fields split into two information tiers:
--   public  — safe for a link posted anywhere (title, summary, area_name).
--   private — full address + building entry instructions, shown only via
--             links whose tier is 'private' (see event_links).
CREATE TABLE events (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    tagline TEXT NOT NULL DEFAULT '',
    starts_at TEXT NOT NULL,
    ends_at TEXT,
    timezone TEXT NOT NULL DEFAULT 'America/Los_Angeles',
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'published', 'archived')),
    -- public tier
    summary TEXT NOT NULL DEFAULT '',
    area_name TEXT NOT NULL DEFAULT '',
    -- private tier
    address TEXT NOT NULL DEFAULT '',
    entry_instructions TEXT NOT NULL DEFAULT '',
    private_details TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (account_id, slug)
);

CREATE INDEX idx_events_account ON events(account_id);
