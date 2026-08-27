//! The cookie, and the two extractors that turn it into a principal.
//!
//! There are three principals and no fourth (`kernel::principal`), so there
//! are exactly two extractors here: [`Visitor`], which resolves the cookie and
//! is happy to find nothing, and [`Session`], which is a `Visitor` that
//! insisted on being somebody. A route picks one, and the choice *is* the
//! authorisation for "signed in at all" — there is no middleware that could be
//! forgotten and no bypass a configuration flag could turn on.
//!
//! Resolving is a read. It never writes: an expiring session is renewed on the
//! observation lane, together with `last_seen`, at most once per session per
//! throttle window ([`rn_kernel::observe`]).

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};
use axum::response::Response;
use rn_kernel::domain::Token;
use rn_kernel::principal::{Principal, Resolved, resolve_cached};

use crate::api::decline;
use crate::config::Mode;
use crate::state::AppState;

/// The session cookie's name. One name, everywhere.
pub const COOKIE: &str = "rn_session";

/// Whoever is asking, signed in or not.
///
/// Infallible by construction: not being signed in is not an error, and the
/// route decides whether it cares.
#[derive(Debug, Clone)]
pub struct Visitor {
    /// Who they are.
    pub principal: Principal,
    /// When the session stops working. Zero when there is none.
    pub expires_at: i64,
}

/// A signed-in member. Anything else is the uniform decline.
#[derive(Debug, Clone)]
pub struct Session {
    /// Who they are — always [`Principal::Member`].
    pub principal: Principal,
    /// When the session stops working.
    pub expires_at: i64,
}

impl<S> FromRequestParts<S> for Visitor
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        Ok(resolve(&state, &parts.headers).await)
    }
}

impl<S> FromRequestParts<S> for Session
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let visitor = Visitor::from_request_parts(parts, state).await?;
        match visitor.principal {
            principal @ Principal::Member { .. } => Ok(Self {
                principal,
                expires_at: visitor.expires_at,
            }),
            _ => Err(decline::forbidden()),
        }
    }
}

/// Resolve a request's cookie into a principal, warm cache first.
///
/// A database that will not answer produces [`Principal::Anonymous`] rather
/// than a 500: from the caller's side an unreachable session store and an
/// expired session are the same event, and the routes above already know what
/// to do with "not signed in".
pub async fn resolve(state: &AppState, headers: &HeaderMap) -> Visitor {
    let Some(raw) = cookie(headers, COOKIE) else {
        return Visitor {
            principal: Principal::Anonymous,
            expires_at: 0,
        };
    };
    let token = Token::from_wire(raw);
    let resolved = resolve_cached(&state.store.reads(), &state.principals, &token)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "a session could not be resolved");
            Resolved {
                principal: Principal::Anonymous,
                expires_at: 0,
                wants_renewal: false,
            }
        });

    // The sighting goes on the async lane; the read that noticed it does not
    // write, and a full queue drops the timestamp rather than growing.
    if let Some(session) = resolved.principal.session() {
        state
            .observations
            .touch(session, state.store.clock().now(), resolved.wants_renewal);
    }

    Visitor {
        principal: resolved.principal,
        expires_at: resolved.expires_at,
    }
}

/// Forget the warm resolution of the cookie this request carried.
///
/// The feed invalidator ([`crate::sub::invalidate`]) evicts every node's cache
/// a moment later; this is the caller's own, evicted before its own response
/// leaves, so the browser that just signed out cannot beat the notification
/// back with a second request.
pub fn forget(state: &AppState, headers: &HeaderMap) {
    if let Some(raw) = cookie(headers, COOKIE) {
        state.principals.forget(&Token::from_wire(raw).digest());
    }
}

/// One cookie's value out of a `Cookie` header.
#[must_use]
pub fn cookie<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())?
        .split(';')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| key.trim() == name)
        .map(|(_, value)| value.trim())
}

/// The `Set-Cookie` value that starts a session.
///
/// `HttpOnly` so script cannot read it, `SameSite=Lax` so a top-level
/// navigation from elsewhere still arrives signed in while a cross-site form
/// post does not, `Secure` in production only — a dev server on loopback has
/// no TLS and a `Secure` cookie there is a cookie that is never sent.
#[must_use]
pub fn set(token: &Token, max_age: i64, mode: Mode) -> String {
    format!(
        "{COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        token.expose(),
        max_age.max(0),
        secure(mode),
    )
}

/// The `Set-Cookie` value that ends one.
#[must_use]
pub fn clear(mode: Mode) -> String {
    format!(
        "{COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        secure(mode)
    )
}

const fn secure(mode: Mode) -> &'static str {
    match mode {
        Mode::Prod => "; Secure",
        Mode::Dev => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(cookie: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(cookie).unwrap());
        headers
    }

    #[test]
    fn the_session_cookie_is_found_among_its_neighbours() {
        let jar = headers("rn_theme=dark; rn_session=abc123; other=1");
        assert_eq!(cookie(&jar, COOKIE), Some("abc123"));
        assert_eq!(cookie(&jar, "rn_theme"), Some("dark"));
        assert_eq!(cookie(&jar, "absent"), None);
        assert_eq!(cookie(&HeaderMap::new(), COOKIE), None);
    }

    #[test]
    fn production_marks_the_cookie_secure_and_dev_cannot_afford_to() {
        let token = Token::from_wire("a-fake-token");
        let prod = set(&token, 100, Mode::Prod);
        assert!(prod.contains("; Secure"), "{prod}");
        assert!(prod.contains("HttpOnly"));
        assert!(prod.contains("SameSite=Lax"));
        assert!(prod.starts_with("rn_session=a-fake-token; Path=/"));
        assert!(!set(&token, 100, Mode::Dev).contains("Secure"));
    }

    #[test]
    fn clearing_expires_the_cookie_on_the_same_path_it_was_set() {
        let cleared = clear(Mode::Prod);
        assert!(cleared.contains("Max-Age=0"));
        assert!(cleared.contains("Path=/"));
        assert!(cleared.contains("; Secure"));
    }
}
