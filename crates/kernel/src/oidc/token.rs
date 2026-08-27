//! Access and refresh tokens: opaque, hashed at rest, and bound to the session
//! that produced them.
//!
//! **Opaque, not a JWT.** A self-contained access token is a token nobody can
//! revoke: the resource server verifies a signature and asks nothing, so the
//! only thing that ends it is its own expiry. Here the token is 256 random
//! bits and every use is a lookup, which is what makes "sign out" mean the
//! same thing at every relying party as it does here.
//!
//! **Bound to the session.** `session_id` is NOT NULL for a user token — the
//! schema's CHECK says so — and ending a session revokes its tokens in the
//! same transaction. Nothing sweeps them later and nothing has to remember to:
//! it is one statement in the batch that ends the session.
//!
//! **Refresh rotation with family revocation.** Each refresh mints a new pair
//! and revokes the one presented. Presenting a rotated-away refresh token is
//! evidence that somebody has a copy, so the whole family dies — the legitimate
//! holder and the thief both lose, which is the outcome RFC 9700 §4.14.2 asks
//! for, because the alternative is that only the thief keeps working.

use crate::Timestamp;
use crate::bind;
use crate::domain::Token;
use crate::error::Outcome;
use crate::ids::{Id, OidcClient, OidcToken, Person, Session};
use crate::store::{Cursor, FromRow, Reads, RowError};

/// How long an access token lasts. Short, because it is the thing that
/// travels: an hour of a leaked token is an hour, and the refresh token is
/// what makes that survivable.
pub const ACCESS_TTL: i64 = 60 * 60;

/// How long a refresh token lasts. Thirty days, and it dies with the session
/// long before that in every ordinary case.
pub const REFRESH_TTL: i64 = 60 * 60 * 24 * 30;

/// One `oidc_token` row.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenRow {
    /// The rowid.
    pub id: Id<OidcToken>,
    /// `access` or `refresh`.
    pub kind: String,
    /// Which client holds it.
    pub client_id: Id<OidcClient>,
    /// The session it dies with. `None` only for a service token.
    pub session_id: Option<Id<Session>>,
    /// The person it speaks for. `None` for a service token.
    pub person_id: Option<Id<Person>>,
    /// The service party it speaks as. `None` for a user token.
    pub service_party_id: Option<Id<Person>>,
    /// The first refresh token of its rotation chain.
    pub family_id: Option<i64>,
    /// The `scope` string it carries.
    pub scopes: String,
    /// The RP's replay nonce, for the id token a refresh mints.
    pub nonce: Option<String>,
    /// When the session last proved a factor.
    pub auth_time: Option<Timestamp>,
    /// When it stops working.
    pub expires_at: Timestamp,
    /// When it was revoked, if it was.
    pub revoked_at: Option<Timestamp>,
}

impl TokenRow {
    /// Whether this token still works at `now`.
    #[must_use]
    pub const fn is_live(&self, now: Timestamp) -> bool {
        self.revoked_at.is_none() && self.expires_at > now
    }
}

impl FromRow for TokenRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            kind: row.text("kind")?,
            client_id: row.id("client_id")?,
            session_id: row.id_opt("session_id")?,
            person_id: row.id_opt("person_id")?,
            service_party_id: row.id_opt("service_party_id")?,
            family_id: row.int_opt("family_id")?,
            scopes: row.text("scopes")?,
            nonce: row.text_opt("nonce")?,
            auth_time: row.int_opt("auth_time")?,
            expires_at: row.int("expires_at")?,
            revoked_at: row.int_opt("revoked_at")?,
        })
    }
}

/// The columns [`TokenRow`] reads.
pub const COLUMNS: &str = "id, kind, client_id, session_id, person_id, service_party_id, \
     family_id, scopes, nonce, auth_time, expires_at, revoked_at";

/// One token by its digest — the hot lookup, a seek on the UNIQUE index.
pub const BY_HASH_SQL: &str = "SELECT id, kind, client_id, session_id, person_id, \
     service_party_id, family_id, scopes, nonce, auth_time, expires_at, revoked_at \
     FROM oidc_token WHERE token_hash = $1";

/// Mint a token.
pub const INSERT_SQL: &str = "INSERT INTO oidc_token \
     (token_hash, kind, client_id, session_id, person_id, service_party_id, family_id, scopes, \
      nonce, auth_time, expires_at, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id";

/// Fill in a refresh token's own id as its family, for the first of a chain.
pub const SEED_FAMILY_SQL: &str = "UPDATE oidc_token SET family_id = id WHERE id = $1";

/// Revoke one token by rowid.
pub const REVOKE_ONE_SQL: &str =
    "UPDATE oidc_token SET revoked_at = $1 WHERE id = $2 AND revoked_at IS NULL";

/// Revoke a whole rotation family — what a replayed refresh token costs.
pub const REVOKE_FAMILY_SQL: &str =
    "UPDATE oidc_token SET revoked_at = $1 WHERE family_id = $2 AND revoked_at IS NULL";

/// Take every token a session minted with it.
///
/// A delete rather than a revocation, and the schema is what decides: ending a
/// session *deletes* the `session` row (`cmd::sign_out`), and
/// `oidc_token.session_id` references it, so a token left behind would be a
/// foreign key pointing at nothing. Which is the right outcome anyway — the
/// row's only remaining job would be to say "no" to a lookup, and an absent
/// row already says that.
pub const DELETE_SESSION_TOKENS_SQL: &str = "DELETE FROM oidc_token WHERE session_id = $1";

/// The same for the codes a session minted and nobody redeemed.
pub const DELETE_SESSION_CODES_SQL: &str = "DELETE FROM oidc_code WHERE session_id = $1";

/// The same, for every session of every identity resolved onto one person —
/// what `Disable` cascades to.
pub const DELETE_PERSON_TOKENS_SQL: &str = "DELETE FROM oidc_token WHERE session_id IN      (SELECT s.id FROM session s JOIN identity i ON i.id = s.identity_id WHERE i.person_id = $1)";

/// And the codes.
pub const DELETE_PERSON_CODES_SQL: &str = "DELETE FROM oidc_code WHERE session_id IN      (SELECT s.id FROM session s JOIN identity i ON i.id = s.identity_id WHERE i.person_id = $1)";

/// Revoke every token one person holds at one client — what withdrawing a
/// consent costs.
pub const REVOKE_CONSENT_SQL: &str = "UPDATE oidc_token SET revoked_at = $1 \
     WHERE client_id = $2 AND person_id = $3 AND revoked_at IS NULL";

/// Drop tokens that expired long enough ago to be of no further interest.
pub const SWEEP_SQL: &str = "DELETE FROM oidc_token WHERE expires_at <= $1";

/// The clients that hold a live token under one session, and where to tell
/// them it ended.
///
/// This is what a `SessionEnded` event carries: the back-channel logout POSTs
/// go to exactly the clients that were given something under this session, and
/// to no others — an RP that never saw this person is not told they left.
pub const LOGOUT_TARGETS_SQL: &str = "SELECT DISTINCT c.id AS id, \
     c.backchannel_logout_uri AS uri \
     FROM oidc_token t JOIN oidc_client c ON c.id = t.client_id \
     WHERE t.session_id = $1 AND c.backchannel_logout_uri IS NOT NULL \
       AND c.deleted_at IS NULL";

/// One client to notify, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogoutTarget {
    /// The client.
    pub client_id: Id<OidcClient>,
    /// Its registered back-channel endpoint.
    pub uri: String,
}

impl FromRow for LogoutTarget {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            client_id: row.id("id")?,
            uri: row.text("uri")?,
        })
    }
}

/// The same question about a person: every client holding a token under any
/// session of any of their registrations. What `Disable` has to tell.
pub const LOGOUT_TARGETS_FOR_PERSON_SQL: &str = "SELECT DISTINCT c.id AS id,      c.backchannel_logout_uri AS uri      FROM oidc_token t JOIN oidc_client c ON c.id = t.client_id      WHERE t.person_id = $1 AND c.backchannel_logout_uri IS NOT NULL        AND c.deleted_at IS NULL";

/// Which clients a session's end has to be told to.
///
/// Read *before* the batch that ends the session, because the batch takes the
/// rows this reads with it. The ids go into the audit payload, so the event a
/// follower sees carries them too and the POSTs can happen anywhere off the
/// request path.
pub async fn logout_targets(
    store: &impl Reads,
    session: Id<Session>,
) -> Outcome<Vec<LogoutTarget>> {
    Ok(store
        .query::<LogoutTarget>(LOGOUT_TARGETS_SQL, bind![session])
        .await?)
}

/// The same, for a person whose party is being disabled.
pub async fn logout_targets_of_person(
    store: &impl Reads,
    person: Id<Person>,
) -> Outcome<Vec<LogoutTarget>> {
    Ok(store
        .query::<LogoutTarget>(LOGOUT_TARGETS_FOR_PERSON_SQL, bind![person])
        .await?)
}

/// The client ids of a target list, as the JSON array an audit payload binds.
#[must_use]
pub fn target_ids_json(targets: &[LogoutTarget]) -> String {
    let ids: Vec<i64> = targets.iter().map(|t| t.client_id.get()).collect();
    serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_owned())
}

/// Read a token by the secret a caller presented.
pub async fn by_secret(store: &impl Reads, secret: &str) -> Outcome<Option<TokenRow>> {
    let digest = Token::from_wire(secret).digest();
    Ok(store
        .query_opt::<TokenRow>(BY_HASH_SQL, bind![digest.to_vec()])
        .await?)
}

/// The tokens one session minted, for the page that lists them.
pub const BY_SESSION_SQL: &str = "SELECT t.id, t.kind, t.client_id, t.scopes, t.expires_at, \
     t.revoked_at, t.created_at, c.client_name \
     FROM oidc_token t JOIN oidc_client c ON c.id = t.client_id \
     WHERE t.session_id = $1 ORDER BY t.id DESC";

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> TokenRow {
        TokenRow {
            id: Id::new(1),
            kind: "access".to_owned(),
            client_id: Id::new(1),
            session_id: Some(Id::new(1)),
            person_id: Some(Id::new(1)),
            service_party_id: None,
            family_id: None,
            scopes: "openid".to_owned(),
            nonce: None,
            auth_time: Some(100),
            expires_at: 3700,
            revoked_at: None,
        }
    }

    #[test]
    fn a_token_stops_being_live_at_its_expiry_or_the_moment_it_is_revoked() {
        let token = row();
        assert!(token.is_live(3699));
        assert!(!token.is_live(3700));
        let revoked = TokenRow {
            revoked_at: Some(200),
            ..row()
        };
        assert!(!revoked.is_live(300), "and revocation does not wait");
        assert!(!revoked.is_live(100), "not even before it happened");
    }
}
