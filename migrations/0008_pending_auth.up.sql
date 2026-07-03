-- pending_auth = the single cross-request state table for every
-- multi-step auth flow (OAuth `state`, magic-link token, QR nonce,
-- email-verification token): a hashed one-time token, a kind, a JSON
-- payload, an expiry. Not used by the phase-1 factors (password/api_token
-- are single-step) but created now so later phases don't need a new
-- table. Sweep expired/consumed rows opportunistically on insert.
CREATE TABLE pending_auth (
    id INTEGER PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    factor_kind TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT '{}',
    identity_id INTEGER REFERENCES identities(id),
    account_id INTEGER REFERENCES accounts(id),
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);
