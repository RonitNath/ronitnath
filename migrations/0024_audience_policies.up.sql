CREATE TABLE audience_policies (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    subject_type TEXT NOT NULL CHECK (subject_type IN ('event', 'calendar_entry')),
    subject_id INTEGER NOT NULL,
    public_level TEXT NOT NULL DEFAULT 'hidden'
        CHECK (public_level IN ('hidden', 'busy', 'summary', 'full')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(subject_type, subject_id)
);

-- Preserve pre-visibility browsing semantics for legacy public links.
INSERT INTO audience_policies (account_id, subject_type, subject_id, public_level)
SELECT e.account_id, 'event', e.id,
       CASE WHEN EXISTS (
           SELECT 1 FROM event_links l
           WHERE l.account_id = e.account_id AND l.event_id = e.id
             AND l.tier = 'public'
       ) THEN 'summary' ELSE 'hidden' END
FROM events e;
