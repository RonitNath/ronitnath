//! CSRF: a per-session synchronizer token, checked against every mutating
//! request. Plain HTML forms carry it as a hidden field (`csrf_field()` on
//! the Askama context, checked against a `Form<T>` field in the handler);
//! the guestbook JSON API carries it as an `X-CSRF-Token` header (set by
//! `ts/src/lib/api.ts` from a `<meta>` tag) since it has no HTML form to
//! embed a field in.
//!
//! Deliberately not a router middleware: a mutating handler almost always
//! needs the request body anyway (`Form<T>`/`Json<T>`), and a body can
//! only be consumed once — buffering it in middleware just to peek at one
//! field, then reconstructing the request for the handler, is more
//! moving parts than checking it inline where the body's already parsed.

use crate::auth::extract::AccountScope;
use crate::error::AppError;

/// `None` submitted-token means "not authenticated via cookie" — bearer
/// tokens have no CSRF token to check at all (see [`AccountScope`]'s doc
/// comment on why that's safe), so `scope.csrf_token` being `None` always
/// passes.
pub fn verify(scope: &AccountScope, submitted: &str) -> Result<(), AppError> {
    match &scope.csrf_token {
        None => Ok(()),
        Some(expected) if constant_time_eq(expected, submitted) => Ok(()),
        Some(_) => Err(AppError::Forbidden("missing or invalid CSRF token".into())),
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
