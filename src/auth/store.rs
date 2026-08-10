//! Durable auth operations against hiqlite.
//!
//! Everything the request path needs from the database lives here, so the
//! routes stay free of SQL and the guard has exactly one way to resolve a
//! session. Every function is `#[instrument]`ed and emits a `latency_ms` event
//! on the way out — that is the raw material the test harness aggregates.

use std::time::Instant;

use hiqlite::{Client, Error as HiqliteError, Row, params};
use tracing::{debug, info, instrument, warn};

use super::capability::{Capability, CapabilitySet};
use super::ids::{InternalId, PublicId};
use super::login::run_auth_gates;
use super::model::{
    AccountKind, AccountStatus, IdentityEmail, IdentityKind, IdentityStatus, MembershipRole,
    normalize_email,
};
use super::password::hash_password;
use super::session::{SESSION_TTL_MS, now_ms, token_hash};

/// A single `id` column coming back from `RETURNING id`.
struct RowId(i64);

impl From<&mut Row<'_>> for RowId {
    fn from(row: &mut Row<'_>) -> Self {
        Self(row.get("id"))
    }
}

/// Who a resolved session belongs to, and what it may do.
#[derive(Clone, Debug)]
pub struct SessionContext {
    pub identity_id: InternalId,
    pub account_id: InternalId,
    pub identity_public_id: PublicId,
    pub account_public_id: PublicId,
    pub capabilities: CapabilitySet,
}

/// The identity/account pair created by registration.
#[derive(Clone, Debug)]
pub struct Registration {
    pub identity_id: InternalId,
    pub account_id: InternalId,
    pub identity_public_id: PublicId,
}

/// Register an identity, its primary account, and the `owner` membership
/// joining them. Default capabilities are not rows — they are implied by the
/// role via [`MembershipRole::implied`].
///
/// Not a transaction: hiqlite serializes writes through raft, and a partial
/// registration would need compensating deletes that do not exist yet. The
/// unique index on `email_normalized` is what makes a retry safe — a second
/// attempt fails at the first insert rather than orphaning an account.
#[instrument(name = "auth.register", skip(db, password), fields(email_normalized))]
pub async fn register(
    db: &Client,
    email: &str,
    password: &str,
) -> Result<Registration, StoreError> {
    let started = Instant::now();
    let normalized = normalize_email(email);
    tracing::Span::current().record("email_normalized", normalized.as_str());

    if normalized.is_empty() || !normalized.contains('@') {
        debug!("rejecting registration: address is not an email");
        return Err(StoreError::InvalidEmail);
    }
    if password.len() < 8 {
        debug!(
            len = password.len(),
            "rejecting registration: password too short"
        );
        return Err(StoreError::WeakPassword);
    }

    if find_email(db, &normalized).await?.is_some() {
        debug!("rejecting registration: address already registered");
        return Err(StoreError::EmailTaken);
    }

    let now = now_ms();
    let identity_public_id = PublicId::new_v4();

    let hash_started = Instant::now();
    let phc = hash_password(password).map_err(|e| StoreError::Password(e.to_string()))?;
    debug!(
        latency_ms = hash_started.elapsed().as_secs_f64() * 1000.0,
        "argon2id hash computed"
    );

    // Identities land `active`: there is no verification mail to wait on yet,
    // and `pending` would lock every new account out of its own session.
    let identity_id: RowId = db
        .execute_returning_map_one(
            "INSERT INTO identities (public_id, kind, display_name, status, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
            params!(
                identity_public_id.to_string_lossy(),
                IdentityKind::Person.as_str(),
                Option::<String>::None,
                IdentityStatus::Active.as_str(),
                now,
                now
            ),
        )
        .await?;
    let identity_id = InternalId::new(identity_id.0);

    db.execute(
        "INSERT INTO identity_emails
           (identity_id, email, email_normalized, verified_at, is_primary, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 1, $5, $6)",
        params!(
            identity_id.get(),
            email.trim().to_string(),
            normalized.clone(),
            Option::<i64>::None,
            now,
            now
        ),
    )
    .await?;

    db.execute(
        "INSERT INTO identity_passwords (identity_id, password_hash, created_at, rotated_at, updated_at)
         VALUES ($1, $2, $3, $4, $5)",
        params!(identity_id.get(), phc, now, now, now),
    )
    .await?;

    let account_public_id = PublicId::new_v4();
    let account_id: RowId = db
        .execute_returning_map_one(
            "INSERT INTO accounts
               (public_id, kind, name, status, primary_for_identity_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
            params!(
                account_public_id.to_string_lossy(),
                AccountKind::Primary.as_str(),
                normalized.clone(),
                AccountStatus::Active.as_str(),
                identity_id.get(),
                now,
                now
            ),
        )
        .await?;
    let account_id = InternalId::new(account_id.0);

    db.execute(
        "INSERT INTO account_memberships (account_id, identity_id, role, created_at)
         VALUES ($1, $2, $3, $4)",
        params!(
            account_id.get(),
            identity_id.get(),
            MembershipRole::Owner.as_str(),
            now
        ),
    )
    .await?;

    // No capability rows are written here. The membership's role *is* the
    // grant: `resolve_session` unions `MembershipRole::implied()` with the
    // exception rows, so default capabilities cost nothing at registration and
    // follow the role bundle when it changes.
    info!(
        identity_id = identity_id.get(),
        account_id = account_id.get(),
        role = MembershipRole::Owner.as_str(),
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "registration complete"
    );

    Ok(Registration {
        identity_id,
        account_id,
        identity_public_id,
    })
}

/// Row shape for `identity_emails` lookups.
#[derive(serde::Deserialize)]
struct EmailRow {
    id: i64,
    identity_id: i64,
    email: String,
    email_normalized: String,
    verified_at: Option<i64>,
    is_primary: i64,
    created_at: i64,
    updated_at: i64,
}

impl From<EmailRow> for IdentityEmail {
    fn from(row: EmailRow) -> Self {
        Self {
            id: InternalId::new(row.id),
            identity_id: InternalId::new(row.identity_id),
            email: row.email,
            email_normalized: row.email_normalized,
            verified_at: row.verified_at,
            is_primary: row.is_primary != 0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[instrument(name = "auth.find_email", skip(db))]
async fn find_email(db: &Client, normalized: &str) -> Result<Option<IdentityEmail>, StoreError> {
    let started = Instant::now();
    let row: Option<EmailRow> = db
        .query_as_optional(
            "SELECT id, identity_id, email, email_normalized, verified_at, is_primary,
                    created_at, updated_at
             FROM identity_emails WHERE email_normalized = $1",
            params!(normalized.to_string()),
        )
        .await?;
    debug!(
        found = row.is_some(),
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "email lookup finished"
    );
    Ok(row.map(Into::into))
}

#[derive(serde::Deserialize)]
struct PhcRow {
    password_hash: String,
}

#[derive(serde::Deserialize)]
struct AccountRow {
    id: i64,
    public_id: String,
}

/// Verify a credential and, on success, return the identity/account to sign in.
///
/// Returns `Ok(None)` for every failure a caller is allowed to observe —
/// unknown address, wrong password, disabled identity, missing account. The
/// caller renders one decline for all of them; distinguishing them here would
/// only give the route a way to leak the difference.
#[instrument(name = "auth.authenticate", skip(db, password))]
pub async fn authenticate(
    db: &Client,
    email: &str,
    password: &str,
) -> Result<Option<Registration>, StoreError> {
    let started = Instant::now();
    let normalized = normalize_email(email);
    let email_row = find_email(db, &normalized).await?;

    let phc = match &email_row {
        Some(row) => db
            .query_as_optional::<PhcRow, _>(
                "SELECT password_hash FROM identity_passwords WHERE identity_id = $1",
                params!(row.identity_id.get()),
            )
            .await?
            .map(|r| r.password_hash),
        None => None,
    };

    // Runs the email-verification gate and always spends an argon2 verify —
    // against a dummy PHC when the address is unknown — so an unregistered
    // address costs the same wall-clock as a registered one.
    let verified = run_auth_gates(email_row.as_ref(), password, phc.as_deref());

    let Some(email_row) = email_row else {
        debug!("authenticate declined: unknown address");
        return Ok(None);
    };
    if !verified {
        debug!("authenticate declined: password mismatch");
        return Ok(None);
    }

    let account: Option<AccountRow> = db
        .query_as_optional(
            "SELECT a.id, a.public_id
             FROM accounts a
             JOIN identities i ON i.id = a.primary_for_identity_id
             WHERE a.primary_for_identity_id = $1
               AND a.status = 'active'
               AND i.status = 'active'",
            params!(email_row.identity_id.get()),
        )
        .await?;

    let Some(account) = account else {
        warn!(
            identity_id = email_row.identity_id.get(),
            "credential verified but no active primary account; declining"
        );
        return Ok(None);
    };

    let identity_public_id: PublicId = db
        .query_as_optional::<AccountRow, _>(
            "SELECT id, public_id FROM identities WHERE id = $1",
            params!(email_row.identity_id.get()),
        )
        .await?
        .and_then(|r| r.public_id.parse().ok())
        .ok_or(StoreError::Corrupt(
            "identity row missing or unparseable public_id",
        ))?;

    info!(
        identity_id = email_row.identity_id.get(),
        account_id = account.id,
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "authenticate succeeded"
    );

    Ok(Some(Registration {
        identity_id: email_row.identity_id,
        account_id: InternalId::new(account.id),
        identity_public_id,
    }))
}

/// Insert a session row and return the *plaintext* token for the cookie.
///
/// The token is returned once and never stored; only its hash is persisted.
#[instrument(name = "auth.create_session", skip(db, user_agent))]
pub async fn create_session(
    db: &Client,
    registration: &Registration,
    user_agent: Option<String>,
) -> Result<String, StoreError> {
    let started = Instant::now();
    let token = super::session::mint_token();
    let hash = token_hash(&token);
    let now = now_ms();

    db.execute(
        "INSERT INTO sessions
           (token_hash, identity_id, account_id, created_at, expires_at, last_seen_at,
            user_agent)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        params!(
            hash,
            registration.identity_id.get(),
            registration.account_id.get(),
            now,
            now + SESSION_TTL_MS,
            now,
            user_agent
        ),
    )
    .await?;

    info!(
        identity_id = registration.identity_id.get(),
        account_id = registration.account_id.get(),
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "session created"
    );
    Ok(token)
}

#[derive(serde::Deserialize)]
struct SessionRow {
    identity_id: i64,
    account_id: i64,
    expires_at: i64,
    identity_public_id: String,
    account_public_id: String,
    role: String,
}

#[derive(serde::Deserialize)]
struct CapabilityRow {
    capability: String,
}

/// Resolve a cookie token into a [`SessionContext`], or `None` if it is unknown,
/// expired, or points at an identity/account no longer active.
///
/// Revocation is absence: there is no revoked flag to test, because a revoked
/// session has no row. Expiry is the one remaining way a row outlives its
/// usefulness, so an expired row found here is deleted on the spot rather than
/// left for a sweep that does not exist yet.
#[instrument(name = "auth.resolve_session", skip(db, token))]
pub async fn resolve_session(
    db: &Client,
    token: &str,
) -> Result<Option<SessionContext>, StoreError> {
    let started = Instant::now();
    let hash = token_hash(token);

    // The membership join is what ties the session to a role: an inner join,
    // so a session whose membership was deleted stops resolving in the same
    // breath — access revocation and session revocation cannot drift apart.
    let row: Option<SessionRow> = db
        .query_as_optional(
            "SELECT s.identity_id, s.account_id, s.expires_at,
                    i.public_id AS identity_public_id,
                    a.public_id AS account_public_id,
                    m.role
             FROM sessions s
             JOIN identities i ON i.id = s.identity_id
             JOIN accounts   a ON a.id = s.account_id
             JOIN account_memberships m
               ON m.account_id = s.account_id AND m.identity_id = s.identity_id
             WHERE s.token_hash = $1
               AND i.status = 'active'
               AND a.status = 'active'",
            params!(hash.clone()),
        )
        .await?;

    let Some(row) = row else {
        debug!(
            latency_ms = started.elapsed().as_secs_f64() * 1000.0,
            "session token did not resolve"
        );
        return Ok(None);
    };

    let now = now_ms();
    if row.expires_at <= now {
        // Delete rather than merely decline: with no tombstone column, an
        // expired row is indistinguishable from a live one except by this
        // comparison, and leaving it behind means `sessions` only ever grows.
        if let Err(err) = db
            .execute(
                "DELETE FROM sessions WHERE token_hash = $1",
                params!(hash.clone()),
            )
            .await
        {
            warn!(%err, "could not delete an expired session; declining anyway");
        }
        debug!(
            expires_at = row.expires_at,
            "session expired; deleted and treating as anonymous"
        );
        return Ok(None);
    }

    let role: MembershipRole = row
        .role
        .parse()
        .map_err(|()| StoreError::Corrupt("membership role is not a known role"))?;

    // Role bundle ∪ exception rows. Unknown capability strings are dropped by
    // `resolve` — log them here so a retired capability's leftover rows are
    // visible rather than silently dead weight.
    let explicit: Vec<String> = db
        .query_as::<CapabilityRow, _>(
            "SELECT capability FROM membership_capabilities
             WHERE account_id = $1 AND identity_id = $2",
            params!(row.account_id, row.identity_id),
        )
        .await?
        .into_iter()
        .map(|r| r.capability)
        .collect();
    for unknown in explicit.iter().filter(|c| Capability::parse(c).is_none()) {
        warn!(capability = %unknown, "ignoring unknown capability row");
    }
    let capabilities = CapabilitySet::resolve(role, explicit.iter().map(String::as_str));

    // Best-effort liveness stamp; a failure here must not fail the request.
    if let Err(err) = db
        .execute(
            "UPDATE sessions SET last_seen_at = $1 WHERE token_hash = $2",
            params!(now, hash),
        )
        .await
    {
        warn!(%err, "could not update session last_seen_at");
    }

    debug!(
        identity_id = row.identity_id,
        account_id = row.account_id,
        capabilities = %capabilities.to_log_string(),
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "session resolved"
    );

    Ok(Some(SessionContext {
        identity_id: InternalId::new(row.identity_id),
        account_id: InternalId::new(row.account_id),
        identity_public_id: row
            .identity_public_id
            .parse()
            .map_err(|_| StoreError::Corrupt("identity public_id is not a uuid"))?,
        account_public_id: row
            .account_public_id
            .parse()
            .map_err(|_| StoreError::Corrupt("account public_id is not a uuid"))?,
        capabilities,
    }))
}

/// Revoke a session by deleting its row.
///
/// No tombstone: a revoked session is one that does not exist. That keeps the
/// resolver's condition to a single lookup, and it means a leaked `sessions`
/// dump cannot even tell you that a session was once held.
///
/// Idempotent — deleting an absent row reports `false` rather than erroring, so
/// a double sign-out and a sign-out with a stale cookie behave identically.
#[instrument(name = "auth.revoke_session", skip(db, token))]
pub async fn revoke_session(db: &Client, token: &str) -> Result<bool, StoreError> {
    let started = Instant::now();
    let hash = token_hash(token);
    let affected = db
        .execute("DELETE FROM sessions WHERE token_hash = $1", params!(hash))
        .await?;
    info!(
        revoked = affected,
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "session revoke finished"
    );
    Ok(affected > 0)
}

/// Revoke every session held by one identity, across all of its accounts.
///
/// Uses `sessions_membership_idx` by its leading column. This is what a password
/// change or a "sign out everywhere" calls; without the delete-not-tombstone
/// rule it would have to stamp every matching row instead.
#[instrument(name = "auth.revoke_sessions_for_identity", skip(db))]
pub async fn revoke_sessions_for_identity(
    db: &Client,
    identity_id: InternalId,
) -> Result<usize, StoreError> {
    let affected = db
        .execute(
            "DELETE FROM sessions WHERE identity_id = $1",
            params!(identity_id.get()),
        )
        .await?;
    info!(revoked = affected, "revoked every session for identity");
    Ok(affected)
}

/// Revoke every session for one `(account_id, identity_id)` membership.
///
/// The membership key exists for exactly this: revoking a person's access to
/// one account must not sign them out of the others.
#[instrument(name = "auth.revoke_sessions_for_membership", skip(db))]
pub async fn revoke_sessions_for_membership(
    db: &Client,
    account_id: InternalId,
    identity_id: InternalId,
) -> Result<usize, StoreError> {
    let affected = db
        .execute(
            "DELETE FROM sessions WHERE identity_id = $1 AND account_id = $2",
            params!(identity_id.get(), account_id.get()),
        )
        .await?;
    info!(revoked = affected, "revoked every session for membership");
    Ok(affected)
}

/// The sign-in triple for an address, without checking any credential.
///
/// This is the *credential-free* variant of [`authenticate`]: it exists for
/// callers that have already earned the right to a session some other way —
/// today that is only the debug-build dev bypass. Nothing on the normal
/// request path may call this.
#[instrument(name = "auth.registration_for_email", skip(db))]
pub async fn registration_for_email(
    db: &Client,
    email: &str,
) -> Result<Option<Registration>, StoreError> {
    #[derive(serde::Deserialize)]
    struct TripleRow {
        identity_id: i64,
        account_id: i64,
        identity_public_id: String,
    }
    let row: Option<TripleRow> = db
        .query_as_optional(
            "SELECT e.identity_id, a.id AS account_id,
                    i.public_id AS identity_public_id
             FROM identity_emails e
             JOIN identities i ON i.id = e.identity_id
             JOIN accounts a ON a.primary_for_identity_id = e.identity_id
             WHERE e.email_normalized = $1
               AND i.status = 'active' AND a.status = 'active'",
            params!(normalize_email(email)),
        )
        .await?;
    row.map(|r| {
        Ok(Registration {
            identity_id: InternalId::new(r.identity_id),
            account_id: InternalId::new(r.account_id),
            identity_public_id: r
                .identity_public_id
                .parse()
                .map_err(|_| StoreError::Corrupt("identity public_id is not a uuid"))?,
        })
    })
    .transpose()
}

/// The membership joining an email's identity to its primary account.
///
/// This is how operator tooling names a membership: by the address a person
/// signs in with, not by internal ids. `None` when the address is unknown or
/// the identity has no primary account.
#[instrument(name = "auth.membership_for_email", skip(db))]
pub async fn membership_for_email(
    db: &Client,
    email: &str,
) -> Result<Option<(InternalId, InternalId)>, StoreError> {
    #[derive(serde::Deserialize)]
    struct MembershipRow {
        account_id: i64,
        identity_id: i64,
    }
    let row: Option<MembershipRow> = db
        .query_as_optional(
            "SELECT m.account_id, m.identity_id
             FROM identity_emails e
             JOIN accounts a ON a.primary_for_identity_id = e.identity_id
             JOIN account_memberships m
               ON m.account_id = a.id AND m.identity_id = e.identity_id
             WHERE e.email_normalized = $1",
            params!(normalize_email(email)),
        )
        .await?;
    Ok(row.map(|r| (InternalId::new(r.account_id), InternalId::new(r.identity_id))))
}

/// Grant a capability to an existing membership.
///
/// Nothing on the request path calls this — it is the seam an admin surface
/// will use, and it is what a test uses to prove `/manage` opens once `manage`
/// is actually granted.
#[instrument(name = "auth.grant_capability", skip(db))]
pub async fn grant_capability(
    db: &Client,
    account_id: InternalId,
    identity_id: InternalId,
    capability: Capability,
) -> Result<(), StoreError> {
    db.execute(
        "INSERT OR IGNORE INTO membership_capabilities
           (account_id, identity_id, capability, granted_at)
         VALUES ($1, $2, $3, $4)",
        params!(
            account_id.get(),
            identity_id.get(),
            capability.as_str().to_string(),
            now_ms()
        ),
    )
    .await?;
    info!(capability = capability.as_str(), "capability granted");
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("email address is not usable")]
    InvalidEmail,
    #[error("password does not meet the minimum length")]
    WeakPassword,
    #[error("email address is already registered")]
    EmailTaken,
    #[error("password hashing failed: {0}")]
    Password(String),
    #[error("inconsistent row: {0}")]
    Corrupt(&'static str),
    #[error(transparent)]
    Db(#[from] HiqliteError),
}
