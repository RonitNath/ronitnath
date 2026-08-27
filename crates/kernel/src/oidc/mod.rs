//! The OpenID Provider.
//!
//! This deployment is its own issuer. It has its own keys, its own client
//! registry and its own subjects, and nothing here federates to another
//! deployment: `ronitnath.com` and `isoastra.com` are two providers that have
//! never heard of each other, which is what makes "sign in with your account"
//! mean one account rather than an implicit trust relationship nobody wrote
//! down.
//!
//! ## What is in here
//!
//! * [`client`] — the registry, redirect matching, and client authentication.
//! * [`subject`] — `sub`, pairwise by sector, minted once and never rotated.
//! * [`consent`] — a relation row plus the scopes it agreed to.
//! * [`code`] — authorization codes: single use, sixty seconds, PKCE always.
//! * [`token`] — access and refresh tokens, opaque and bound to a session.
//! * [`key`] — the RS256 keys, sealed at rest, rotated by a command.
//! * [`jwt`] — the one JWT shape this deployment signs and verifies.
//! * [`handle`] — a person's `preferred_username`.
//!
//! ## The three rulings the whole thing turns on
//!
//! **A token dies with its session.** `oidc_token.session_id` is NOT NULL for
//! a user token, and every path that ends a session revokes its tokens in the
//! same transaction. There is no sweeper to fall behind and no RP that keeps
//! working after somebody signs out.
//!
//! **`sub` is pairwise by the client's owner, not by the client.** Two
//! applications of one organization see one person; two organizations cannot
//! correlate at all. It is never an id — an id is a reversible encryption of a
//! rowid, and handing one to a relying party would make the string it stores
//! the string every other endpoint addresses rows by.
//!
//! **Consent is a relation.** `oidc_client:X #authorized @person:Y` goes
//! through the same `check()` as everything else, so there is no second
//! authorisation path to disagree with the first.

pub mod client;
pub mod code;
pub mod consent;
pub mod handle;
pub mod jwt;
pub mod key;
pub mod subject;
pub mod token;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use serde_json::Value as Json;

pub use client::{ClientRow, Credential};
pub use code::CodeRow;
pub use consent::ConsentRow;
pub use key::{KeyRow, SealKey, SigningKey};
pub use token::{LogoutTarget, TokenRow};

/// What the OpenID Provider needs from its deployment, and nothing else does.
///
/// Two facts, and they are both per-deployment: the issuer — this
/// deployment's public origin, which is `iss` *exactly* and is what every
/// endpoint URL in the discovery document is built from — and the key the
/// signing keys' private halves are sealed under.
///
/// It rides on [`crate::cmd::Ctx`] rather than being read from a global,
/// because a command that reached for a process-wide issuer would be a command
/// a test could not run two of.
#[derive(Debug, Clone)]
pub struct Provider {
    issuer: String,
    seal: SealKey,
}

impl Provider {
    /// Build one. `issuer` is the public origin with no trailing slash: `iss`
    /// is compared byte for byte by every relying party, so a stray slash is a
    /// deployment nobody can sign in to.
    #[must_use]
    pub fn new(issuer: impl Into<String>, seal: SealKey) -> Self {
        let issuer = issuer.into();
        Self {
            issuer: issuer.trim_end_matches('/').to_owned(),
            seal,
        }
    }

    /// A provider for a dev process and for a test: loopback, and a sealing
    /// key minted for this process. Never a deployment's — a key minted per
    /// boot is a JWKS that changes per boot.
    #[must_use]
    pub fn dev() -> Self {
        Self::new("http://127.0.0.1:3004", SealKey::mint())
    }

    /// `iss`.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The key private halves are sealed under.
    #[must_use]
    pub const fn seal(&self) -> &SealKey {
        &self.seal
    }

    /// One endpoint's URL, from the issuer.
    #[must_use]
    pub fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.issuer)
    }

    /// The token endpoint — the audience a `private_key_jwt` assertion must
    /// name (RFC 7523 §3), so an assertion cannot be replayed elsewhere.
    #[must_use]
    pub fn token_endpoint(&self) -> String {
        self.endpoint(paths::TOKEN)
    }
}

/// The paths every endpoint lives at, written once so the discovery document
/// and the router cannot disagree about where anything is.
pub mod paths {
    /// OpenID Provider metadata.
    pub const DISCOVERY: &str = "/.well-known/openid-configuration";
    /// The same document under RFC 8414's name.
    pub const OAUTH_METADATA: &str = "/.well-known/oauth-authorization-server";
    /// Where a person consents.
    pub const AUTHORIZE: &str = "/oidc/authorize";
    /// Where a code or a refresh token becomes an access token.
    pub const TOKEN: &str = "/oidc/token";
    /// Where an access token becomes claims.
    pub const USERINFO: &str = "/oidc/userinfo";
    /// The public keys.
    pub const JWKS: &str = "/oidc/jwks";
    /// RFC 7009.
    pub const REVOKE: &str = "/oidc/revoke";
    /// RP-initiated logout.
    pub const END_SESSION: &str = "/oidc/end_session";
}

/// The claim an RP reads a person's chosen name from.
pub const PREFERRED_USERNAME: &str = "preferred_username";

/// The event identifier a logout token's `events` member carries.
pub const LOGOUT_EVENT: &str = "http://schemas.openid.net/event/backchannel-logout";

/// Whether a client's own JWKS document verifies these JWT parts.
///
/// The `kid` narrows the search when the header carries one and every key is
/// tried when it does not — which is what a document with a single unnamed key
/// needs, and is safe because a key that verifies is a key the client
/// published.
#[must_use]
pub fn jwks_verifies(jwks: &str, parts: &jwt::Parts) -> bool {
    let Ok(document): Result<Json, _> = serde_json::from_str(jwks) else {
        return false;
    };
    let Some(keys) = document.get("keys").and_then(Json::as_array) else {
        return false;
    };
    keys.iter()
        .filter(
            |jwk| match (parts.kid(), jwk.get("kid").and_then(Json::as_str)) {
                (Some(wanted), Some(named)) => wanted == named,
                _ => true,
            },
        )
        .filter_map(rsa_of_jwk)
        .any(|public| jwt::verify(&public, parts))
}

/// One RSA public key out of a JWK, or nothing.
fn rsa_of_jwk(jwk: &Json) -> Option<rsa::RsaPublicKey> {
    if jwk.get("kty").and_then(Json::as_str) != Some("RSA") {
        return None;
    }
    let n = B64.decode(jwk.get("n")?.as_str()?).ok()?;
    let e = B64.decode(jwk.get("e")?.as_str()?).ok()?;
    rsa::RsaPublicKey::new(
        rsa::BigUint::from_bytes_be(&n),
        rsa::BigUint::from_bytes_be(&e),
    )
    .ok()
}

/// The JWKS document this deployment publishes.
///
/// `use: sig` and `alg: RS256` on every key, because an RP that has to guess
/// what a key is for will guess.
#[must_use]
pub fn jwks_document(keys: &[KeyRow]) -> Json {
    let members: Vec<Json> = keys
        .iter()
        .map(|key| {
            serde_json::json!({
                "kty": "RSA",
                "use": "sig",
                "alg": jwt::ALG,
                "kid": key.kid,
                "n": key.modulus,
                "e": key.exponent,
            })
        })
        .collect();
    serde_json::json!({ "keys": members })
}

#[cfg(test)]
mod tests;
