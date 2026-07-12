//! Session cookie plumbing: token generation/hashing and cookie building.
//!
//! Sessions themselves (the DB row, expiry, revocation) live in
//! [`crate::store::sessions`] — this module only ever deals with the raw
//! cookie value, never storing it anywhere but the client.

use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

/// `__Host-` cookies require `Secure` and no `Domain` attribute — only
/// usable once the server is actually behind TLS. Local HTTP dev falls
/// back to a plain name.
pub fn cookie_name(secure: bool) -> &'static str {
    if secure { "__Host-session" } else { "session" }
}

/// A fresh 256-bit random token, base64url-encoded for cookie/header use.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// sha256 of a raw token — what's actually stored (`sessions.token_hash`,
/// `factors.secret_hash` for api tokens). Never store the raw value.
pub fn hash_token(token: &str) -> String {
    STANDARD.encode(Sha256::digest(token.as_bytes()))
}

pub fn build_cookie(secure: bool, token: String, ttl_secs: i64) -> Cookie<'static> {
    let mut cookie = Cookie::build((cookie_name(secure), token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(ttl_secs));
    if secure {
        cookie = cookie.secure(true);
    }
    cookie.build()
}

/// The cookie that clears a session on logout — same name/flags as
/// [`build_cookie`] so the browser actually overwrites it, immediately
/// expired.
pub fn clear_cookie(secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::build((cookie_name(secure), ""))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(0));
    if secure {
        cookie = cookie.secure(true);
    }
    cookie.build()
}
