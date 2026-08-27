//! Authorization codes: single use, sixty seconds, PKCE always.
//!
//! A code is a bearer secret like a session token, and it is stored the same
//! way — 256 bits from the OS, only its SHA-256 in the row. What it adds is
//! three bindings, and each one closes a different hole:
//!
//! * the **redirect URI**, so a code minted for one landing place cannot be
//!   redeemed against another (RFC 6749 §4.1.3);
//! * the **PKCE challenge**, so a code intercepted in the front channel is
//!   worthless without the verifier that never left the client (RFC 7636).
//!   It is `NOT NULL` in the schema, because this deployment requires PKCE of
//!   every client type — so a verifier arriving for a code with no challenge
//!   is not a case that can exist, which is the downgrade the check refuses;
//! * the **session**, so the tokens the code becomes die when it does.
//!
//! Single use is the database's job, exactly as it is for an invitation link:
//! the statement that marks a code used carries `used_at IS NULL AND
//! expires_at > now`, and every other statement in the batch carries the same
//! guard, so a code redeemed twice writes nothing the second time.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use sha2::{Digest as _, Sha256};

use crate::Timestamp;
use crate::bind;
use crate::error::Outcome;
use crate::ids::{Id, Identity, OidcClient, OidcCode, Person, Session};
use crate::store::{Cursor, FromRow, Reads, RowError};

/// How long a code lasts. The specification says "a maximum of 10 minutes is
/// RECOMMENDED" (RFC 6749 §4.1.2) and one minute is enough for a redirect: a
/// code alive for ten is nine minutes of a stolen browser history being worth
/// something.
pub const TTL: i64 = 60;

/// One `oidc_code` row.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeRow {
    /// The rowid.
    pub id: Id<OidcCode>,
    /// Which client it was minted for.
    pub client_id: Id<OidcClient>,
    /// Whose it is.
    pub person_id: Id<Person>,
    /// The registration that authenticated.
    pub identity_id: Id<Identity>,
    /// The session it is bound to.
    pub session_id: Id<Session>,
    /// Where the response landed.
    pub redirect_uri: String,
    /// What was consented to, as the `scope` string.
    pub scopes: String,
    /// The RP's replay nonce.
    pub nonce: Option<String>,
    /// The S256 challenge.
    pub code_challenge: String,
    /// When the session last proved a factor.
    pub auth_time: Timestamp,
    /// When the code stops working.
    pub expires_at: Timestamp,
    /// When it was redeemed, if it has been.
    pub used_at: Option<Timestamp>,
}

impl CodeRow {
    /// Whether this code may still be redeemed at `now`.
    #[must_use]
    pub const fn is_live(&self, now: Timestamp) -> bool {
        self.used_at.is_none() && self.expires_at > now
    }
}

impl FromRow for CodeRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            client_id: row.id("client_id")?,
            person_id: row.id("person_id")?,
            identity_id: row.id("identity_id")?,
            session_id: row.id("session_id")?,
            redirect_uri: row.text("redirect_uri")?,
            scopes: row.text("scopes")?,
            nonce: row.text_opt("nonce")?,
            code_challenge: row.text("code_challenge")?,
            auth_time: row.int("auth_time")?,
            expires_at: row.int("expires_at")?,
            used_at: row.int_opt("used_at")?,
        })
    }
}

/// One code by its digest — a seek on the UNIQUE index, which matters because
/// this runs on a route anybody may post a guess to.
pub const BY_HASH_SQL: &str = "SELECT id, client_id, person_id, identity_id, session_id, \
     redirect_uri, scopes, nonce, code_challenge, auth_time, expires_at, used_at \
     FROM oidc_code WHERE code_hash = $1";

/// Mint a code.
pub const INSERT_SQL: &str = "INSERT INTO oidc_code \
     (code_hash, client_id, person_id, identity_id, session_id, redirect_uri, scopes, nonce, \
      code_challenge, auth_time, expires_at, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id";

/// Mark a code redeemed. The guard is the whole of "single use".
pub const REDEEM_SQL: &str = "UPDATE oidc_code SET used_at = $2 \
     WHERE id = $1 AND used_at IS NULL AND expires_at > $2";

/// Drop codes nobody can redeem any more.
pub const SWEEP_SQL: &str = "DELETE FROM oidc_code WHERE expires_at <= $1";

/// Read a code by the secret a caller presented.
pub async fn by_secret(store: &impl Reads, secret: &str) -> Outcome<Option<CodeRow>> {
    Ok(store
        .query_opt::<CodeRow>(BY_HASH_SQL, bind![digest(secret).to_vec()])
        .await?)
}

/// What the row stores instead of the code.
#[must_use]
pub fn digest(secret: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.finalize().into()
}

/// The S256 challenge a verifier produces (RFC 7636 §4.6).
#[must_use]
pub fn challenge_of(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    B64.encode(hasher.finalize())
}

/// Whether a verifier answers a challenge.
///
/// The verifier's own shape is checked first: RFC 7636 §4.1 makes it 43–128
/// characters of the unreserved set, and a client that sends eight characters
/// has not implemented PKCE, it has decorated it.
#[must_use]
pub fn verifier_matches(challenge: &str, verifier: &str) -> bool {
    let shaped = (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'));
    if !shaped {
        return false;
    }
    let computed = challenge_of(verifier);
    // Same length by construction, so a byte-wise fold is constant time over
    // the pair; equality on `String` would return early on the first
    // difference and time the answer.
    if computed.len() != challenge.len() {
        return false;
    }
    computed
        .bytes()
        .zip(challenge.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example of RFC 7636 Appendix B.
    #[test]
    fn the_specifications_own_example_verifies() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(challenge_of(verifier), challenge);
        assert!(verifier_matches(challenge, verifier));
    }

    #[test]
    fn a_wrong_or_malformed_verifier_answers_nothing() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = challenge_of(verifier);
        assert!(!verifier_matches(
            &challenge,
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXj"
        ));
        assert!(
            !verifier_matches(&challenge, "short"),
            "under 43 characters"
        );
        assert!(!verifier_matches(&challenge, &"a".repeat(129)), "over 128");
        assert!(
            !verifier_matches(&challenge, &format!("{}!", &verifier[..42])),
            "outside the unreserved set"
        );
    }

    #[test]
    fn a_code_is_stored_as_its_digest_and_dies_when_it_is_used_or_expires() {
        let secret = crate::domain::Token::mint();
        assert_ne!(
            digest(secret.expose()).as_slice(),
            secret.expose().as_bytes()
        );
        let mut row = CodeRow {
            id: Id::new(1),
            client_id: Id::new(1),
            person_id: Id::new(1),
            identity_id: Id::new(1),
            session_id: Id::new(1),
            redirect_uri: "http://127.0.0.1:4180/cb".to_owned(),
            scopes: "openid".to_owned(),
            nonce: None,
            code_challenge: "c".to_owned(),
            auth_time: 100,
            expires_at: 160,
            used_at: None,
        };
        assert!(row.is_live(100));
        assert!(!row.is_live(160), "expiry is exclusive");
        row.used_at = Some(120);
        assert!(!row.is_live(100));
    }
}
