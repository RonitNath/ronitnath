-- Capability links — the only way a guest reaches an event page.
--   tier 'public'  → page renders public-tier info only.
--   tier 'private' → page additionally renders address + entry instructions
--                    and private-tier schedule items.
--   person_id set  → the link is personalized: the page greets that person
--                    and edits their RSVP directly.
--
-- Lookups go through token_hash (sha256 of the raw token). token_plain is
-- kept deliberately, unlike sessions/api_tokens: these are capability URLs
-- to party info, not account credentials, and the admin workflow (re-copy
-- someone's link weeks later, print a QR) needs the raw value. Revoke and
-- re-mint if one leaks.
CREATE TABLE event_links (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    person_id INTEGER REFERENCES people(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    token_plain TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    tier TEXT NOT NULL DEFAULT 'public'
        CHECK (tier IN ('public', 'private')),
    revoked_at TEXT,
    uses INTEGER NOT NULL DEFAULT 0,
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_event_links_event ON event_links(event_id);
CREATE INDEX idx_event_links_person ON event_links(person_id);
