//! Browser-freshness headers, applied to every response in one place.
//!
//! Two rules, and the split between them is the whole point:
//!
//! * **Documents are `no-store`.** They carry session-shaped state; a
//!   back-button replay out of the bfcache is a stale-permissions bug.
//! * **API responses are `private, no-store`.** Same reason, plus the word
//!   that tells a shared cache between the browser and the node that these
//!   bytes belong to one reader (`docs/rebuild/plan.md` §API).
//! * **Assets are `no-cache, must-revalidate`.** Their URLs are stable across
//!   releases — there is no `?v=` cache-busting anywhere — so the browser must
//!   ask every time and gets a 304 for its trouble.
//!
//! `X-RN-App-Version` rides on everything, which is what makes "which revision
//! is nyc serving" answerable with one `curl -I` against any URL at all.
//!
//! The other thing applied to every route in one place is a **deadline**. A
//! write on a cluster with no quorum does not fail — it waits, for as long as
//! it takes quorum to come back, and the drill measured 24 and 27 seconds of
//! that (finding 3, `docs/perf/2026-08-27.md`). The plan's resilience row asks
//! for "writes refuse with the uniform decline"; without a deadline what a
//! caller gets is a socket that goes quiet.

use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::api::decline;
use crate::state::AppState;

/// How long a write may take before this node refuses it.
///
/// A command is one raft round trip and one transaction, measured in
/// milliseconds; five seconds is not a budget, it is the distance past which
/// the answer is "this cluster cannot commit right now" rather than "your
/// command is slow". It sits above the confirmed-read wait
/// (`api::cmd::confirmed::WAIT`) so that a node catching up answers for
/// itself.
pub const COMMAND_DEADLINE: Duration = Duration::from_secs(5);

/// How long a read may take. Reads are local to the node that serves them and
/// have no quorum to wait for, so a read that has not finished in two seconds
/// is not going to.
pub const QUERY_DEADLINE: Duration = Duration::from_secs(2);

pub const VERSION_HEADER: HeaderName = HeaderName::from_static("x-rn-app-version");

/// The `Cache-Control` value for a path. Asset prefixes are listed rather than
/// inferred from an extension: a route is an asset because we serve it as one,
/// and a `.json` API response must not become cacheable by renaming.
#[must_use]
pub fn cache_policy(path: &str) -> &'static str {
    const ASSETS: &[&str] = &[
        "/static/",
        "/tokens.css",
        "/pkg/",
        "/app/pkg/",
        "/org/pkg/",
        "/platform/pkg/",
        "/favicon.ico",
    ];
    // The OpenID Provider's two public documents. Neither carries a byte of
    // session state — the metadata is the deployment's own description and the
    // JWKS is public keys — and both are fetched by every relying party on
    // every start, so they are the only responses here a shared cache may
    // hold. Five minutes, which is short enough that a key rotation reaches an
    // RP promptly and long enough that a fleet of them is not a thundering
    // herd; the `retiring` status is what makes even a stale copy correct.
    const PUBLIC: &[&str] = &[
        "/.well-known/openid-configuration",
        "/.well-known/oauth-authorization-server",
        "/oidc/jwks",
    ];
    if PUBLIC.contains(&path) {
        return "public, max-age=300";
    }
    if ASSETS.iter().any(|prefix| path.starts_with(prefix)) {
        "no-cache, must-revalidate"
    } else if path.starts_with("/api/") {
        "private, no-store"
    } else {
        "no-store"
    }
}

/// How long this request gets, or `None` for a route that is not on a clock.
///
/// Three rules. `/api/sub` is a WebSocket that lives for as long as the tab
/// does, and a deadline on it is a deadline on the subscription. Anything
/// that is not a read is a command — the `/api/cmd/*` posts and the `/auth`
/// form posts, which go through the same dispatch — and gets
/// [`COMMAND_DEADLINE`]. Every other `/api/` route is a read and gets
/// [`QUERY_DEADLINE`].
///
/// Documents and assets are deliberately not on a clock: a large asset over a
/// slow connection is a long response and not a stuck one, and a deadline
/// there would truncate exactly the downloads that need the time.
#[must_use]
pub fn deadline_for(method: &Method, path: &str) -> Option<Duration> {
    if path == "/api/sub" {
        return None;
    }
    if !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Some(COMMAND_DEADLINE);
    }
    path.starts_with("/api/").then_some(QUERY_DEADLINE)
}

/// Refuse rather than hang.
///
/// The layer is written out here rather than mounted from
/// `tower_http::timeout` for one reason: that layer answers `408 Request
/// Timeout` with an empty body, and both halves are wrong for this surface.
/// The contract is that a refusal is the uniform decline shape
/// (`docs/rebuild/plan.md` §API), and the thing being refused is the
/// *cluster's* inability to commit, which is a `503` (`decline::unavailable`).
///
/// A timed-out command is *undecided*, not undone: the handler's future is
/// dropped, which cancels this node's wait, and a raft entry already in
/// flight may still apply. That is precisely what the idempotency key is
/// for — the caller re-sends the same body with the same key and gets either
/// the original reply or a fresh commit, never two. A `503` is the status
/// that says "unknown"; it is the reason this is not a `409` or a `403`.
pub async fn deadline(request: Request, next: Next) -> Response {
    let Some(budget) = deadline_for(request.method(), request.uri().path()) else {
        return next.run(request).await;
    };
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    match tokio::time::timeout(budget, next.run(request)).await {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!(
                %method,
                %path,
                budget_ms = budget.as_millis() as u64,
                "a request passed its deadline and was refused"
            );
            decline::unavailable()
        }
    }
}

pub async fn freshness(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_policy(&path)),
    );
    headers.insert(
        VERSION_HEADER,
        HeaderValue::from_str(&state.version)
            .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::{COMMAND_DEADLINE, QUERY_DEADLINE, deadline_for};
    use axum::http::Method;

    #[test]
    fn a_command_gets_the_write_budget_and_a_query_the_read_one() {
        assert_eq!(
            deadline_for(&Method::POST, "/api/cmd/edit-document"),
            Some(COMMAND_DEADLINE)
        );
        assert_eq!(deadline_for(&Method::POST, "/auth"), Some(COMMAND_DEADLINE));
        assert_eq!(
            deadline_for(&Method::GET, "/api/q/documents"),
            Some(QUERY_DEADLINE)
        );
        assert_eq!(
            deadline_for(&Method::GET, "/api/whoami"),
            Some(QUERY_DEADLINE)
        );
    }

    #[test]
    fn a_subscription_is_not_on_a_clock_and_neither_is_an_asset() {
        assert_eq!(deadline_for(&Method::GET, "/api/sub"), None);
        assert_eq!(deadline_for(&Method::GET, "/static/stars/bright.bin"), None);
        assert_eq!(deadline_for(&Method::GET, "/app"), None);
        assert_eq!(deadline_for(&Method::GET, "/"), None);
    }
}

#[cfg(test)]
mod cache_tests {
    use super::cache_policy;

    #[test]
    fn assets_revalidate_and_everything_else_refuses_to_be_stored() {
        for asset in [
            "/static/stars/bright.bin",
            "/tokens.css",
            "/pkg/starscape/rn-starscape.js",
            "/app/pkg/rn-app_bg.wasm",
            "/platform/pkg/rn-platform.js",
            "/favicon.ico",
        ] {
            assert_eq!(cache_policy(asset), "no-cache, must-revalidate", "{asset}");
        }
        for public in [
            "/.well-known/openid-configuration",
            "/.well-known/oauth-authorization-server",
            "/oidc/jwks",
        ] {
            assert_eq!(cache_policy(public), "public, max-age=300", "{public}");
        }
        for document in [
            "/",
            "/healthz",
            "/readyz",
            "/version",
            "/auth",
            "/app",
            "/links/x",
            // Everything else the Provider serves is a person's or a client's.
            "/oidc/authorize",
            "/oidc/token",
            "/oidc/userinfo",
            "/oidc/end_session",
        ] {
            assert_eq!(cache_policy(document), "no-store", "{document}");
        }
        for private in [
            "/api/whoami",
            "/api/q/sessions",
            "/api/cmd/sign-out",
            "/api/sub",
        ] {
            assert_eq!(cache_policy(private), "private, no-store", "{private}");
        }
    }
}
