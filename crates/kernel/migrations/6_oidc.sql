-- Migration 6 — this deployment is an OpenID Provider.
--
-- Every deployment is its own issuer, with its own keys and its own client
-- registry; nothing federates between them. What that costs the schema is
-- seven objects and one column, and the column is the interesting one.
--
-- Migrations 1–5 are frozen: hiqlite hashes them, so a column appended to one
-- would invalidate every already-migrated node. Everything new is here.
--
-- ---------------------------------------------------------------- handle ----
-- A person's handle is their `preferred_username`: human-chosen, unique,
-- lowercase `[a-z0-9-]{3,32}`, and *not* an id. It goes on `party` because a
-- person is a party row and a handle is a fact about the human, not about one
-- of their registrations — which is also why a merge keeps the survivor's and
-- files the absorbed one as an alias rather than deleting it. An old handle
-- that stopped resolving would be a URL that used to name somebody.
ALTER TABLE party
    ADD COLUMN handle TEXT;

-- Unique among the handles that exist. Partial, because every organization,
-- group and service party carries NULL here and SQLite would otherwise treat
-- one NULL as distinct from another only by accident of the standard.
CREATE UNIQUE INDEX party_handle_unique_idx ON party (handle) WHERE handle IS NOT NULL;

-- The handles a merge absorbed, and the person they now resolve to. A row
-- here is a handle nobody may take again: `party_handle_unique_idx` does not
-- see it, so the refusal is this table's, checked by the commands that mint.
CREATE TABLE party_handle_alias
(
    handle    TEXT    PRIMARY KEY,
    person_id INTEGER NOT NULL REFERENCES party (id),
    at        INTEGER NOT NULL
);

CREATE INDEX party_handle_alias_person_idx ON party_handle_alias (person_id);

-- ---------------------------------------------------------- oidc_client ----
-- A registered relying party. RFC 7591's metadata shape, without dynamic
-- registration: registering is an operator command, and the shape is the
-- registration request's so that adding the endpoint later adds no migration.
--
-- `owner_party_id` is a person or an organization — never a group, which is
-- the trigger below, and never a service. The owner is also the *sector*: a
-- person's `sub` at this client is pairwise under whoever owns it, so two
-- clients of one organization see one subject and two organizations never do.
--
-- NULL means the deployment itself — `platform:*`, the singleton every
-- operator relation hangs off, which has no `party` row to point at. That is
-- the same shape `relation` already uses for `public` and `authenticated`, and
-- it is why the column is nullable rather than pointing at a seeded "platform"
-- organization nobody founded.
--
-- `secret_hash` is SHA-256 and not argon2id, and that is not a lapse. A client
-- secret is 256 bits minted by this server; there is no dictionary to run
-- against it and no human who chose it, so the only thing a slow hash would
-- buy is a slow token endpoint. It is compared in constant time.
CREATE TABLE oidc_client
(
    id                         INTEGER PRIMARY KEY,
    owner_party_id             INTEGER REFERENCES party (id),
    -- The service party `client_credentials` mints tokens for. One per client,
    -- created with it, owned by the client's owner.
    service_party_id           INTEGER NOT NULL REFERENCES party (id),
    client_name                TEXT    NOT NULL,
    client_uri                 TEXT,
    logo_uri                   TEXT,
    -- JSON arrays. Exact match on redirect, with RFC 8252 §7.3's loopback-port
    -- exception applied in code, where the rule can be stated once.
    redirect_uris              TEXT    NOT NULL,
    post_logout_redirect_uris  TEXT    NOT NULL,
    backchannel_logout_uri     TEXT,
    token_endpoint_auth_method TEXT    NOT NULL CHECK (token_endpoint_auth_method IN
                                                       ('client_secret_basic', 'client_secret_post',
                                                        'private_key_jwt', 'none')),
    -- The client's own JWKS, for `private_key_jwt`.
    jwks                       TEXT,
    grant_types                TEXT    NOT NULL,
    scopes                     TEXT    NOT NULL,
    -- First-party: the consent page is skipped.
    trusted                    INTEGER NOT NULL CHECK (trusted IN (0, 1)),
    -- Refuse a person holding no relation under the owner.
    members_only               INTEGER NOT NULL CHECK (members_only IN (0, 1)),
    secret_hash                BLOB,
    created_at                 INTEGER NOT NULL,
    rotated_at                 INTEGER,
    -- Withdrawal is a tombstone rather than a DELETE: codes, tokens and
    -- consents reference this row, and the audit trail references them.
    deleted_at                 INTEGER
);

CREATE INDEX oidc_client_owner_idx ON oidc_client (owner_party_id, deleted_at);

-- The same sentence migration 1 wrote about resources, about clients: a group
-- is granted things and never owns them. A CHECK cannot see another table.
CREATE TRIGGER oidc_client_owner_is_never_a_group_insert
    BEFORE INSERT
    ON oidc_client
    FOR EACH ROW
    WHEN (SELECT kind FROM party WHERE id = NEW.owner_party_id) IN ('group', 'service')
BEGIN
    SELECT RAISE(ABORT, 'a group never owns an oidc client');
END;

CREATE TRIGGER oidc_client_owner_is_never_a_group_update
    BEFORE UPDATE OF owner_party_id
    ON oidc_client
    FOR EACH ROW
    WHEN (SELECT kind FROM party WHERE id = NEW.owner_party_id) IN ('group', 'service')
BEGIN
    SELECT RAISE(ABORT, 'a group never owns an oidc client');
END;

-- --------------------------------------------------------- oidc_subject ----
-- `sub`, pairwise by sector. One row per (sector party, person), minted once
-- and never rotated: an RP's whole notion of who somebody is rests on this
-- string, so rotating it would silently make one person two.
--
-- The sector is the client's *owner*, not the client, which is what lets an
-- organization's two applications recognise the same person while another
-- organization's cannot correlate at all.
--
-- After a merge the absorbed person's row stays, and both subs still resolve:
-- a sub names a person, and a person resolves through `person_alias`.
-- `sector_party_id` carries `0` for the platform's own clients and a `party`
-- rowid for an organization's. No REFERENCES clause, for the reason
-- `relation.subject_id` has none: `0` names a singleton with no row, and a
-- NULL would make the UNIQUE index below stop deduplicating, since SQLite
-- treats NULLs as distinct.
CREATE TABLE oidc_subject
(
    id              INTEGER PRIMARY KEY,
    sector_party_id INTEGER NOT NULL,
    person_id       INTEGER NOT NULL REFERENCES party (id),
    sub             TEXT    NOT NULL,
    created_at      INTEGER NOT NULL
);

CREATE UNIQUE INDEX oidc_subject_pair_idx ON oidc_subject (sector_party_id, person_id);
CREATE UNIQUE INDEX oidc_subject_sub_idx ON oidc_subject (sub);

-- --------------------------------------------------------- oidc_consent ----
-- What a person agreed to give a client.
--
-- The *grant* is a relation row — `oidc_client:X #authorized @person:Y` — so
-- "has this person authorised this client" is answered by `check()` in the one
-- indexed query everything else uses, with no second authorisation path. This
-- table is the attribute that row cannot carry: which scopes, and when. It is
-- read only after `check()` has already said yes.
CREATE TABLE oidc_consent
(
    client_id INTEGER NOT NULL REFERENCES oidc_client (id),
    person_id INTEGER NOT NULL REFERENCES party (id),
    scopes    TEXT    NOT NULL,
    at        INTEGER NOT NULL,
    PRIMARY KEY (client_id, person_id)
);

-- `authorized` is a relation word, and migration 2's vocabulary trigger did
-- not know it. SQLite cannot alter a trigger, so it is dropped and rewritten;
-- `relation::vocabulary`'s test reads this list back out of `sqlite_master`
-- and compares it with the Rust enum, so the two still cannot drift.
DROP TRIGGER relation_vocabulary_insert;

CREATE TRIGGER relation_vocabulary_insert
    BEFORE INSERT
    ON relation
    FOR EACH ROW
    WHEN NEW.relation NOT IN ('viewer', 'commenter', 'editor', 'owner',
                              'member', 'admin', 'contact', 'operator',
                              'authorized')
BEGIN
    SELECT RAISE(ABORT, 'not a relation this deployment knows');
END;

-- ------------------------------------------------------------ oidc_code ----
-- An authorization code: single use, sixty seconds, bound to the session that
-- produced it.
--
-- `code_challenge` is the S256 challenge and it is NOT NULL, because PKCE is
-- required of every client type here (RFC 9700 §2.1.1) — which also means a
-- verifier arriving for a code with no challenge is not a case this schema can
-- represent, and the downgrade it would be is refused in code as well.
CREATE TABLE oidc_code
(
    id             INTEGER PRIMARY KEY,
    code_hash      BLOB    NOT NULL UNIQUE,
    client_id      INTEGER NOT NULL REFERENCES oidc_client (id),
    person_id      INTEGER NOT NULL REFERENCES party (id),
    identity_id    INTEGER NOT NULL REFERENCES identity (id),
    session_id     INTEGER NOT NULL REFERENCES session (id),
    redirect_uri   TEXT    NOT NULL,
    scopes         TEXT    NOT NULL,
    nonce          TEXT,
    code_challenge TEXT    NOT NULL,
    auth_time      INTEGER NOT NULL,
    expires_at     INTEGER NOT NULL,
    used_at        INTEGER,
    created_at     INTEGER NOT NULL
);

CREATE INDEX oidc_code_expires_idx ON oidc_code (expires_at);

-- ----------------------------------------------------------- oidc_token ----
-- Access and refresh tokens, opaque and hashed at rest — a database read
-- cannot mint one, exactly as with sessions and links.
--
-- `session_id` is NOT NULL for a user token, and that is the binding the whole
-- design turns on: ending the session ends every token minted under it, in the
-- same transaction, so "sign out" means the same thing at every RP as it does
-- here. The two CHECKs say it: a token is a person's *or* a service's, and a
-- person's token has a session.
--
-- `family_id` is the first refresh token of a rotation chain. Presenting a
-- rotated-away refresh token revokes the family (RFC 9700 §4.14.2).
CREATE TABLE oidc_token
(
    id               INTEGER PRIMARY KEY,
    token_hash       BLOB    NOT NULL UNIQUE,
    kind             TEXT    NOT NULL CHECK (kind IN ('access', 'refresh')),
    client_id        INTEGER NOT NULL REFERENCES oidc_client (id),
    session_id       INTEGER REFERENCES session (id),
    person_id        INTEGER REFERENCES party (id),
    service_party_id INTEGER REFERENCES party (id),
    family_id        INTEGER,
    scopes           TEXT    NOT NULL,
    nonce            TEXT,
    auth_time        INTEGER,
    expires_at       INTEGER NOT NULL,
    revoked_at       INTEGER,
    created_at       INTEGER NOT NULL,
    CHECK ((person_id IS NULL) <> (service_party_id IS NULL)),
    CHECK ((session_id IS NULL) = (person_id IS NULL))
);

-- The hot lookup: a bearer token arrives, and this is the only way to find it.
CREATE INDEX oidc_token_session_idx ON oidc_token (session_id, revoked_at);
CREATE INDEX oidc_token_family_idx ON oidc_token (family_id);
CREATE INDEX oidc_token_person_client_idx ON oidc_token (person_id, client_id);
CREATE INDEX oidc_token_expires_idx ON oidc_token (expires_at);

-- ------------------------------------------------------------- oidc_key ----
-- The RS256 signing keys. `kid` is what a JWT header names and what an RP
-- looks up in the JWKS; three statuses, and the middle one is the whole point
-- of having them — a `retiring` key still verifies while tokens signed under
-- it are alive, and stops when they are not.
--
-- The private half is sealed at rest with AES-256-GCM under a key that is not
-- in the database: `RN_SITE__OIDC_KEY`, or `RN_SITE__OIDC_KEY_FILE`. A
-- separate key from the id key on purpose — the id key names rows and this one
-- forges identity, and one variable that does both is one leak that does both.
CREATE TABLE oidc_key
(
    id          INTEGER PRIMARY KEY,
    kid         TEXT    NOT NULL UNIQUE,
    status      TEXT    NOT NULL CHECK (status IN ('active', 'retiring', 'retired')),
    -- The public half, as the two base64url JWK members.
    modulus     TEXT    NOT NULL,
    exponent    TEXT    NOT NULL,
    -- The sealed private half: a 12-byte nonce followed by the ciphertext.
    sealed      BLOB    NOT NULL,
    created_at  INTEGER NOT NULL,
    retired_at  INTEGER
);

CREATE INDEX oidc_key_status_idx ON oidc_key (status, id DESC);

-- --------------------------------------------------------- jti registry ----
-- `private_key_jwt` assertions are single use: an assertion replayed is a
-- client credential replayed. Rows are swept with the codes.
CREATE TABLE oidc_assertion
(
    jti        TEXT    PRIMARY KEY,
    client_id  INTEGER NOT NULL REFERENCES oidc_client (id),
    expires_at INTEGER NOT NULL
);

CREATE INDEX oidc_assertion_expires_idx ON oidc_assertion (expires_at);
