//! Session tokens and the cookie that carries them.
//!
//! The token the browser holds has roughly 244 bits of UUID-v4 randomness. The
//! database stores only its SHA-256 digest, so a database dump contains no
//! replayable bearer credential. Lookup is by digest, which is why the column
//! is `UNIQUE`.
//!
//! There is no signed-cookie crate here on purpose: the cookie value is an
//! opaque bearer token that is worthless unless it matches a row, so signing it
//! would only re-prove what the database lookup already proves.

use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tracing::debug;
use uuid::Uuid;

use crate::operations::config::Mode;

/// Cookie name carrying the session token.
pub const SESSION_COOKIE: &str = "rn_session";

/// Session lifetime. Short enough that an abandoned session expires on its own.
pub const SESSION_TTL_MS: i64 = 14 * 24 * 60 * 60 * 1000;

/// Unix millis. One place so every timestamp column agrees.
#[must_use]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after 1970")
        .as_millis() as i64
}

/// Mint a fresh opaque session token (two v4 UUIDs, hex, no dashes).
#[must_use]
pub fn mint_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// What goes in `sessions.token_hash`. Never log the input.
#[must_use]
pub fn token_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Whether to mark the session cookie `Secure`.
///
/// A named type rather than a bare `bool` because the two call sites are a
/// `Set-Cookie` and a clearing `Set-Cookie`, and getting the argument backwards
/// at either one is silent: the cookie still works, it just also travels over
/// plaintext. `Prod` is derived from the runtime mode in
/// [`AuthState::for_mode`](crate::auth::AuthState::for_mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieSecurity {
    /// Omit `Secure`: dev over plain HTTP, and the test transport, which is not
    /// HTTPS and would silently drop a `Secure` cookie from its jar.
    Insecure,
    /// Send `Secure`: the browser must never put this cookie on a plaintext
    /// request, even one the edge would have redirected.
    Secure,
}

impl CookieSecurity {
    /// `"; Secure"` or nothing.
    #[must_use]
    const fn attribute(self) -> &'static str {
        match self {
            Self::Secure => "; Secure",
            Self::Insecure => "",
        }
    }

    #[must_use]
    pub const fn is_secure(self) -> bool {
        matches!(self, Self::Secure)
    }

    /// The release setting: `Secure` in prod, plaintext-tolerant otherwise.
    ///
    /// Keyed on the runtime [`Mode`] rather than on `debug_assertions`, because
    /// what decides whether the cookie can be `Secure` is whether there is TLS
    /// in front of the process — a deployment fact — not which cargo profile
    /// compiled it. `cargo run --release` against a local HTTP listener stays
    /// signable-in; a prod node never sends the cookie in the clear.
    #[must_use]
    pub const fn for_mode(mode: Mode) -> Self {
        match mode {
            Mode::Prod => Self::Secure,
            Mode::Dev => Self::Insecure,
        }
    }
}

/// `Set-Cookie` value that installs a session.
///
/// `HttpOnly` keeps it away from island JS, `SameSite=Lax` lets a normal
/// top-level navigation back to the site keep the session while still blocking
/// cross-site form posts. `Secure` is under `security` because the edge
/// terminates TLS in prod but dev and the test transport are plain HTTP, where a
/// `Secure` cookie would never be sent back at all.
#[must_use]
pub fn set_cookie_header(token: &str, security: CookieSecurity) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax{}; Max-Age={}",
        security.attribute(),
        SESSION_TTL_MS / 1000
    )
}

/// `Set-Cookie` value that removes a session cookie.
///
/// Carries the same attributes as [`set_cookie_header`] so the clear is a clean
/// overwrite of what was set rather than a second cookie beside it.
#[must_use]
pub fn clear_cookie_header(security: CookieSecurity) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax{}; Max-Age=0",
        security.attribute()
    )
}

/// Pull the session token out of a `Cookie` header value.
///
/// Tolerates the usual `a=1; b=2` packing and ignores cookies we do not own.
#[must_use]
pub fn token_from_cookie_header(header: &str) -> Option<String> {
    for pair in header.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            if value.is_empty() {
                debug!("session cookie present but empty");
                return None;
            }
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_high_entropy_hex() {
        let token = mint_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(token, mint_token());
    }

    #[test]
    fn hash_is_stable_and_not_the_bearer_token() {
        let token = mint_token();
        assert_eq!(token_hash(&token), token_hash(&token));
        assert_ne!(token_hash(&token), token);
        assert_ne!(token_hash(&token), token_hash(&mint_token()));
    }

    #[test]
    fn cookie_round_trips_out_of_a_packed_header() {
        let token = mint_token();
        let header = format!("theme=dark; {SESSION_COOKIE}={token}; other=1");
        assert_eq!(token_from_cookie_header(&header), Some(token));
    }

    #[test]
    fn missing_or_empty_cookie_is_none() {
        assert_eq!(token_from_cookie_header("theme=dark"), None);
        assert_eq!(
            token_from_cookie_header(&format!("{SESSION_COOKIE}=")),
            None
        );
    }

    #[test]
    fn clear_header_expires_immediately() {
        assert!(clear_cookie_header(CookieSecurity::Insecure).contains("Max-Age=0"));
    }

    #[test]
    fn secure_attribute_follows_the_flag() {
        let token = mint_token();
        assert!(set_cookie_header(&token, CookieSecurity::Secure).contains("; Secure"));
        assert!(!set_cookie_header(&token, CookieSecurity::Insecure).contains("Secure"));
        // The clear must match the set, or the browser keeps both.
        assert!(clear_cookie_header(CookieSecurity::Secure).contains("; Secure"));
        assert!(!clear_cookie_header(CookieSecurity::Insecure).contains("Secure"));
    }

    #[test]
    fn prod_mode_is_the_only_thing_that_turns_secure_on() {
        use crate::operations::config::Mode;
        assert_eq!(CookieSecurity::for_mode(Mode::Prod), CookieSecurity::Secure);
        assert_eq!(
            CookieSecurity::for_mode(Mode::Dev),
            CookieSecurity::Insecure
        );
    }
}
