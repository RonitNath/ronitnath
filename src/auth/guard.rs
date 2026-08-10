//! The capability guard: cookie → session → capability → handler.
//!
//! One middleware does the whole check so there is exactly one place where a
//! request becomes authenticated. It distinguishes two failures, and the
//! distinction is deliberate:
//!
//! - **401** — no usable session. The caller may fix this by signing in.
//! - **403** — a valid session that lacks the capability. Signing in again
//!   changes nothing; someone has to grant it.
//!
//! Collapsing both into 401 would send a signed-in user back to the login form
//! forever. Collapsing both into 403 would hide that a session had expired.

use std::time::Instant;

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{Html, IntoResponse, Response},
};
use tracing::{debug, instrument, warn};

use super::capability::Capability;
use super::session::token_from_cookie_header;
use super::store::{SessionContext, resolve_session};
use crate::auth::AuthState;

/// Resolve the session cookie, require `capability`, and hand off to the route.
///
/// On success the [`SessionContext`] is put in the request extensions so the
/// handler can read who it is serving without a second database round trip.
#[instrument(
    name = "auth.guard",
    skip(state, request, next),
    fields(capability, outcome)
)]
pub async fn require_capability(
    capability: Capability,
    State(state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let span = tracing::Span::current();
    span.record("capability", capability.as_str());

    let token = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(token_from_cookie_header);

    let Some(token) = token else {
        span.record("outcome", "anonymous");
        debug!(
            latency_ms = started.elapsed().as_secs_f64() * 1000.0,
            "guard denied: no session cookie"
        );
        return unauthorized();
    };

    let session = match resolve_session(&state.db, &token).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            span.record("outcome", "unresolved");
            debug!(
                latency_ms = started.elapsed().as_secs_f64() * 1000.0,
                "guard denied: cookie did not resolve to a live session"
            );
            return unauthorized();
        }
        Err(err) => {
            // A database fault is ours, not the caller's. Say 503 rather than
            // 401 so a failing database is never reported as a bad credential.
            span.record("outcome", "error");
            warn!(%err, "guard failed: session lookup errored");
            return service_unavailable();
        }
    };

    if !session.capabilities.has(capability) {
        span.record("outcome", "forbidden");
        debug!(
            identity_id = session.identity_id.get(),
            held = %session.capabilities.to_log_string(),
            latency_ms = started.elapsed().as_secs_f64() * 1000.0,
            "guard denied: session lacks capability"
        );
        return forbidden(capability);
    }

    span.record("outcome", "allowed");
    debug!(
        identity_id = session.identity_id.get(),
        account_id = session.account_id.get(),
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "guard allowed"
    );

    request.extensions_mut().insert(session);
    next.run(request).await
}

/// The [`SessionContext`] the guard attached. Only present behind a guard.
#[must_use]
pub fn session_of(request: &Request) -> Option<&SessionContext> {
    request.extensions().get::<SessionContext>()
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Html(page(
            "Not signed in",
            "This page needs a session. <a href=\"/auth\">Sign in</a> and try again.",
        )),
    )
        .into_response()
}

fn forbidden(capability: Capability) -> Response {
    (
        StatusCode::FORBIDDEN,
        Html(page(
            "Not permitted",
            &format!(
                "Your session is valid but does not hold the <code>{capability}</code> \
                 capability. Signing in again will not change that."
            ),
        )),
    )
        .into_response()
}

fn service_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Html(page(
            "Temporarily unavailable",
            "The session store could not be reached. Try again shortly.",
        )),
    )
        .into_response()
}

/// Minimal standalone document, styled to match the app shell's tokens.
pub(crate) fn page(title: &str, body_html: &str) -> String {
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"/>\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>\
         <title>{title}</title>\
         <style>\
           :root {{ color-scheme: dark light; }}\
           body {{ font: 16px/1.5 system-ui, sans-serif; margin: 0; \
                   min-height: 100vh; display: grid; place-items: center; }}\
           main {{ max-width: 34rem; padding: 2rem; }}\
           code {{ font-family: ui-monospace, monospace; }}\
         </style></head>\
         <body><main><h1>{title}</h1><p>{body_html}</p>\
         <p><a href=\"/\">Home</a></p></main></body></html>"
    )
}
