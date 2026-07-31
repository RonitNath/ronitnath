-- session = an authenticated browser/agent context: identity + the
-- currently active account. token_hash is sha256 of the raw cookie value
-- (never store the raw token). csrf_token is a second random value, handed
-- to the client in forms/meta tags and compared on mutating requests — a
-- session-scoped synchronizer token, not derived from token_hash so leaking
-- one doesn't leak the other. Sliding expiry: last_seen_at advances on use;
-- expires_at is the absolute cutoff.
CREATE TABLE sessions (
    id INTEGER PRIMARY KEY NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    csrf_token TEXT NOT NULL,
    identity_id INTEGER NOT NULL REFERENCES identities(id),
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at TEXT,
    user_agent TEXT,
    ip TEXT
);

CREATE INDEX idx_sessions_identity ON sessions(identity_id);
