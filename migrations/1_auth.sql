-- Auth schema v1.
--
-- Integer `id` is the only join key inside the server. UUID v4 `public_id` is
-- the only identifier that may leave the process. Clients never see `id`.
-- Timestamps are unix millis supplied by the application (no now()/random()).

CREATE TABLE identities
(
    id           INTEGER PRIMARY KEY NOT NULL,
    public_id    TEXT    NOT NULL UNIQUE,
    kind         TEXT    NOT NULL
        CHECK (kind IN ('person', 'service')),
    display_name TEXT,
    status       TEXT    NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'active', 'disabled')),
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE identity_emails
(
    id               INTEGER PRIMARY KEY NOT NULL,
    identity_id      INTEGER NOT NULL REFERENCES identities (id),
    email            TEXT    NOT NULL,
    email_normalized TEXT    NOT NULL UNIQUE,
    -- Kept for the verification gate; verification itself is not implemented yet.
    verified_at      INTEGER,
    is_primary       INTEGER NOT NULL DEFAULT 0
        CHECK (is_primary IN (0, 1)),
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);

CREATE INDEX identity_emails_identity_id_idx ON identity_emails (identity_id);

CREATE TABLE identity_passwords
(
    identity_id INTEGER PRIMARY KEY NOT NULL REFERENCES identities (id),
    -- Complete PHC string: algorithm, version, cost parameters, salt, and output.
    -- Keep this representation intact so old credentials remain verifiable after
    -- changing the current password-hashing policy or its implementation library.
    password_hash TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    rotated_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE accounts
(
    id         INTEGER PRIMARY KEY NOT NULL,
    public_id  TEXT    NOT NULL UNIQUE,
    kind       TEXT    NOT NULL
        CHECK (kind IN ('primary', 'business', 'shared', 'alternate', 'service')),
    name       TEXT    NOT NULL,
    status     TEXT    NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'disabled', 'suspended')),
    -- Set only for kind = 'primary': the identity this primary account belongs to.
    -- Enforces ≤1 primary account per identity via the partial unique index below.
    primary_for_identity_id INTEGER UNIQUE REFERENCES identities (id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK (
        (kind = 'primary' AND primary_for_identity_id IS NOT NULL)
        OR (kind != 'primary' AND primary_for_identity_id IS NULL)
    )
);

CREATE TABLE account_memberships
(
    account_id  INTEGER NOT NULL REFERENCES accounts (id),
    identity_id INTEGER NOT NULL REFERENCES identities (id),
    role        TEXT    NOT NULL
        CHECK (role IN ('owner', 'admin', 'member')),
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (account_id, identity_id)
);

CREATE INDEX account_memberships_identity_id_idx ON account_memberships (identity_id);

CREATE TABLE membership_capabilities
(
    account_id  INTEGER NOT NULL,
    identity_id INTEGER NOT NULL,
    capability  TEXT    NOT NULL,
    granted_at  INTEGER NOT NULL,
    PRIMARY KEY (account_id, identity_id, capability),
    FOREIGN KEY (account_id, identity_id)
        REFERENCES account_memberships (account_id, identity_id)
);

-- A session belongs to a *membership*, not to an identity alone: the same
-- person signed into two accounts holds two sessions with different
-- capabilities. The composite foreign key is what makes that structural — a
-- session can only name a pair that `account_memberships` already joins, which
-- is the same pair `membership_capabilities` is keyed by.
--
-- There is no `revoked_at`. Revocation deletes the row; a session that does not
-- exist is invalid, and that is the only rule the resolver needs.
CREATE TABLE sessions
(
    id          INTEGER PRIMARY KEY NOT NULL,
    -- SHA-256 of the high-entropy opaque bearer token. The plaintext exists
    -- only in the browser cookie and transient request memory.
    token_hash  TEXT    NOT NULL UNIQUE,
    identity_id INTEGER NOT NULL,
    account_id  INTEGER NOT NULL,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    user_agent  TEXT,
    FOREIGN KEY (account_id, identity_id)
        REFERENCES account_memberships (account_id, identity_id)
);

-- Membership key: every "sessions of this identity on this account" query, and
-- — because `identity_id` leads — every "all sessions of this identity" query
-- for a sign-out-everywhere or a password change.
CREATE INDEX sessions_membership_idx ON sessions (identity_id, account_id);
-- Account-first, for revoking a whole account's sessions when it is disabled.
CREATE INDEX sessions_account_id_idx ON sessions (account_id, identity_id);
-- Now that expiry is the only way a row goes stale on its own, a sweep needs
-- to find expired rows without scanning the table.
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);
