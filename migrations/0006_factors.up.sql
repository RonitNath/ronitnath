-- factor = a pluggable login mechanism attached to an identity. Many per
-- identity; proving one proves the identity. external_id is the lookup key
-- for the "find the identity" half of login: the provider subject (OIDC
-- sub, OAuth user id) for federated kinds, or the normalized email for
-- password (so UNIQUE(kind, external_id) also enforces one identity per
-- email). api_token leaves it NULL and is looked up by secret_hash instead
-- — sqlite treats NULL external_id as distinct per row, so that never
-- collides. secret_hash holds the argon2 hash (password) or sha256 of the
-- raw token (api_token); NULL for redirect-based kinds. Tokens/secrets are
-- always stored hashed — raw values exist only in the cookie/email/QR/
-- Authorization header.
CREATE TABLE factors (
    id INTEGER PRIMARY KEY NOT NULL,
    identity_id INTEGER NOT NULL REFERENCES identities(id),
    kind TEXT NOT NULL,
    external_id TEXT,
    secret_hash TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    verified_at TEXT,
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(kind, external_id)
);

CREATE INDEX idx_factors_identity ON factors(identity_id);
CREATE INDEX idx_factors_secret_hash ON factors(secret_hash);
