CREATE TABLE person_identity_links (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    identity_id INTEGER NOT NULL REFERENCES identities(id),
    claimed_at TEXT NOT NULL DEFAULT (datetime('now')),
    unlinked_at TEXT
);

CREATE UNIQUE INDEX idx_pil_active_person
    ON person_identity_links(person_id) WHERE unlinked_at IS NULL;
CREATE UNIQUE INDEX idx_pil_active_identity
    ON person_identity_links(identity_id) WHERE unlinked_at IS NULL;
