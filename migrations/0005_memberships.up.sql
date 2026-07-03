-- membership = the edge between identity and account. Role lives here and
-- nowhere else: "is X an admin" is only well-formed per-account.
CREATE TABLE memberships (
    id INTEGER PRIMARY KEY NOT NULL,
    identity_id INTEGER NOT NULL REFERENCES identities(id),
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(identity_id, account_id)
);

CREATE INDEX idx_memberships_identity ON memberships(identity_id);
CREATE INDEX idx_memberships_account ON memberships(account_id);
