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
    argon2_phc  TEXT    NOT NULL,
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

CREATE TABLE sessions
(
    id          INTEGER PRIMARY KEY NOT NULL,
    token_hash  TEXT    NOT NULL UNIQUE,
    identity_id INTEGER NOT NULL REFERENCES identities (id),
    account_id  INTEGER NOT NULL REFERENCES accounts (id),
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    revoked_at  INTEGER,
    user_agent  TEXT
);

CREATE INDEX sessions_identity_id_idx ON sessions (identity_id);
CREATE INDEX sessions_account_id_idx ON sessions (account_id);
