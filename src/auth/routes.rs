//! The auth HTTP surface: credential endpoints plus a compact guarded-page
//! harness retained for focused integration tests.

use std::time::Instant;

use axum::{
    Form, Json, Router,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{Next, from_fn, from_fn_with_state},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, warn};

use super::capability::Capability;
use super::guard::{require_capability, session_of};
use super::session::{
    SESSION_COOKIE, clear_cookie_header, set_cookie_header, token_from_cookie_header,
};
use super::store::{self, StoreError};
use crate::auth::AuthState;

/// Credential submission shared by register and sign-in.
#[derive(Debug, Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct StatusBody {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

impl StatusBody {
    fn plain(status: &'static str) -> Self {
        Self {
            status,
            identity: None,
        }
    }
}

/// Every auth route, with the guards already attached.
///
/// `/protected` demands `test-auth`, which every role implies; `/manage` — the
/// data browser in [`crate::manage`] — demands `manage`, which no role
/// registration creates. The same signed-in session passes one guard and
/// fails the other until `manage` is granted (role change or explicit row).
pub fn router(state: AuthState) -> Router {
    let protected = Router::new()
        .route("/protected", get(protected_page))
        .route_layer(from_fn_with_state(state.clone(), |st, req, next| {
            require_capability(Capability::TestAuth, st, req, next)
        }));
    let manage = Router::new()
        .route("/manage", get(crate::manage::index))
        .route("/manage/{model}", get(crate::manage::model_page))
        .route_layer(from_fn_with_state(state.clone(), |st, req, next| {
            require_capability(Capability::Manage, st, req, next)
        }));

    api_router(state.clone())
        .merge(protected.with_state(state.clone()))
        .merge(manage.with_state(state))
        .layer(from_fn(no_store))
}

/// Authentication APIs without the legacy standalone guarded page renderers.
/// The production application mounts guarded Leptos renderers for those URLs;
/// integration tests may continue using [`router`] as a compact auth harness.
pub fn api_router(state: AuthState) -> Router {
    let router = Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/signin", post(signin))
        .route("/api/auth/signout", post(signout))
        .route("/api/whoami", get(whoami));

    // The dev bypass exists only in debug builds: a release binary contains
    // no route, no handler, and no reference to it — negative proof, not a
    // disabled flag. The handler re-checks the runtime mode on top.
    #[cfg(debug_assertions)]
    let router = router.route("/api/auth/dev-bypass", post(dev_bypass));

    router.with_state(state).layer(from_fn(no_store))
}

/// Behind the `test-auth` guard: reachable by any registered account.
#[instrument(name = "route.protected", skip(request))]
async fn protected_page(request: Request) -> Html<String> {
    let session = session_of(&request).expect("guard attaches the session before the handler");
    Html(super::guard::page(
        "Protected",
        &format!(
            "Signed in as <code>{}</code> on account <code>{}</code>.<br/>\
             Capabilities held: <code>{}</code>.",
            session.identity_public_id,
            session.account_public_id,
            session.capabilities.to_log_string()
        ),
    ))
}

/// Authentication responses must never be retained by a browser or shared
/// intermediary: they contain session cookies or user-specific authorization
/// results. This layer also covers guard-generated 401/403 responses.
async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static cache-control value"),
    );
    response.headers_mut().insert(
        header::PRAGMA,
        "no-cache".parse().expect("static pragma value"),
    );
    response
}

/// Create an identity, its primary account, and the default capability grants.
///
/// Deliberately does **not** establish a session. Registration and sign-in are
/// separate steps so the sign-in path is exercised by real users on their first
/// visit rather than only on their second.
#[instrument(name = "route.register", skip(state, form), fields(status))]
async fn register(
    State(state): State<AuthState>,
    Form(form): Form<Credentials>,
) -> impl IntoResponse {
    let started = Instant::now();
    let span = tracing::Span::current();

    let response = match store::register(&state.db, &form.email, &form.password).await {
        Ok(registration) => {
            span.record("status", "created");
            (
                StatusCode::CREATED,
                Json(StatusBody {
                    status: "registered",
                    identity: Some(registration.identity_public_id.to_string_lossy()),
                }),
            )
        }
        Err(StoreError::EmailTaken) => {
            span.record("status", "conflict");
            (
                StatusCode::CONFLICT,
                Json(StatusBody::plain("already-registered")),
            )
        }
        Err(err @ (StoreError::InvalidEmail | StoreError::WeakPassword)) => {
            span.record("status", "rejected");
            debug!(%err, "registration rejected");
            (StatusCode::BAD_REQUEST, Json(StatusBody::plain("rejected")))
        }
        Err(err) => {
            span.record("status", "error");
            warn!(%err, "registration failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StatusBody::plain("error")),
            )
        }
    };

    info!(
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "register handled"
    );
    response
}

/// Verify a credential and install a session cookie.
///
/// Every decline is the same 401 and the same body. The store already spends an
/// argon2 verify on unknown addresses, so the failures are also indistinguishable
/// by timing.
#[instrument(name = "route.signin", skip(state, headers, form), fields(status))]
async fn signin(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Form(form): Form<Credentials>,
) -> Response {
    let started = Instant::now();
    let span = tracing::Span::current();

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let outcome = store::authenticate(&state.db, &form.email, &form.password).await;

    let response = match outcome {
        Ok(Some(registration)) => {
            match store::create_session(&state.db, &registration, user_agent).await {
                Ok(token) => {
                    span.record("status", "ok");
                    (
                        StatusCode::OK,
                        [(
                            header::SET_COOKIE,
                            set_cookie_header(&token, state.cookie_security),
                        )],
                        Json(StatusBody {
                            status: "signed-in",
                            identity: Some(registration.identity_public_id.to_string_lossy()),
                        }),
                    )
                        .into_response()
                }
                Err(err) => {
                    span.record("status", "error");
                    warn!(%err, "session creation failed after a valid credential");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(StatusBody::plain("error")),
                    )
                        .into_response()
                }
            }
        }
        Ok(None) => {
            span.record("status", "declined");
            (
                StatusCode::UNAUTHORIZED,
                Json(StatusBody::plain("declined")),
            )
                .into_response()
        }
        Err(err) => {
            span.record("status", "error");
            warn!(%err, "authenticate failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StatusBody::plain("error")),
            )
                .into_response()
        }
    };

    info!(
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "signin handled"
    );
    response
}

/// Revoke the current session and clear the cookie.
///
/// Always 200, and always sends the clearing `Set-Cookie` — including when the
/// caller had no session at all. Sign-out is the one operation that should never
/// fail in a way that leaves a browser holding a cookie it believes in.
#[instrument(name = "route.signout", skip(state, headers), fields(revoked))]
async fn signout(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let started = Instant::now();
    let span = tracing::Span::current();

    let token = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(token_from_cookie_header);

    let revoked = match token {
        Some(token) => match store::revoke_session(&state.db, &token).await {
            Ok(revoked) => revoked,
            Err(err) => {
                warn!(%err, "session revoke failed; still clearing the cookie");
                false
            }
        },
        None => false,
    };
    span.record("revoked", revoked);

    info!(
        revoked,
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "signout handled"
    );

    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            clear_cookie_header(state.cookie_security),
        )],
        Json(StatusBody::plain("signed-out")),
    )
        .into_response()
}

/// Address of the identity the dev bypass signs in as. Debug builds only.
#[cfg(debug_assertions)]
const DEV_BYPASS_EMAIL: &str = "dev-admin@local.test";

/// Mint a `manage`-capable session with no credential. Debug builds only.
///
/// Defense in depth on top of the `#[cfg(debug_assertions)]` compile gate:
/// unless the runtime mode is explicitly `dev`, this answers 404 — the same
/// as if the route had never been compiled in, which in a release binary it
/// isn't. First use creates the `dev-admin@local.test` identity with a
/// random, never-revealed password, so the bypass session is a real session
/// over a real membership and every downstream invariant still holds.
#[cfg(debug_assertions)]
#[instrument(name = "route.dev_bypass", skip(state, headers), fields(status))]
async fn dev_bypass(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    use crate::operations::config::Mode;

    if state.mode != Mode::Dev {
        tracing::Span::current().record("status", "hidden");
        return StatusCode::NOT_FOUND.into_response();
    }

    let registration = match store::registration_for_email(&state.db, DEV_BYPASS_EMAIL).await {
        Ok(Some(registration)) => registration,
        Ok(None) => {
            // A throwaway credential nobody ever sees: the bypass *is* the
            // sign-in path for this identity.
            let password = super::session::mint_token();
            match store::register(&state.db, DEV_BYPASS_EMAIL, &password).await {
                Ok(registration) => registration,
                Err(err) => {
                    warn!(%err, "dev bypass could not create its identity");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
        Err(err) => {
            warn!(%err, "dev bypass lookup failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Idempotent (`INSERT OR IGNORE`), so it also repairs a wiped grant.
    if let Err(err) = store::grant_capability(
        &state.db,
        registration.account_id,
        registration.identity_id,
        Capability::Manage,
    )
    .await
    {
        warn!(%err, "dev bypass could not grant manage");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    match store::create_session(&state.db, &registration, user_agent).await {
        Ok(token) => {
            tracing::Span::current().record("status", "minted");
            info!("dev bypass session minted");
            (
                StatusCode::SEE_OTHER,
                [
                    (
                        header::SET_COOKIE,
                        set_cookie_header(&token, state.cookie_security),
                    ),
                    (header::LOCATION, "/manage".to_string()),
                ],
            )
                .into_response()
        }
        Err(err) => {
            warn!(%err, "dev bypass session creation failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The narrow session picture the client chrome may hold.
///
/// This is the *entire* contract between the server's session state and any
/// user-facing chrome ("signed in as…", nav visibility): public ids and
/// display names only — no emails, no internal ids, no membership role, no
/// session identifier. Widening it is an API decision, not a convenience.
#[derive(Debug, Serialize)]
struct WhoamiBody {
    identity: WhoamiIdentity,
    account: WhoamiAccount,
    /// Wire names of every capability the session holds. A closed set the
    /// server resolved; the client draws chrome from it and proves nothing
    /// with it — the guard re-checks on every request.
    capabilities: Vec<&'static str>,
    /// Unix millis when the session stops resolving.
    expires_at: i64,
}

#[derive(Debug, Serialize)]
struct WhoamiIdentity {
    public_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct WhoamiAccount {
    public_id: String,
    name: String,
    kind: &'static str,
}

/// `GET /api/whoami`: the session context, for user chrome.
///
/// Not behind a capability guard on purpose: it reports *whether and as whom*
/// the caller is signed in, which every session may ask regardless of what it
/// can do. Anonymous callers get a stable 401 JSON shape so signed-out chrome
/// renders from the same call. The router-wide `no_store` layer covers both
/// answers.
#[instrument(name = "route.whoami", skip(state, headers), fields(status))]
async fn whoami(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let span = tracing::Span::current();

    let token = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(token_from_cookie_header);

    let session = match token {
        Some(token) => match store::resolve_session(&state.db, &token).await {
            Ok(session) => session,
            Err(err) => {
                span.record("status", "error");
                warn!(%err, "whoami session lookup errored");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(StatusBody::plain("unavailable")),
                )
                    .into_response();
            }
        },
        None => None,
    };

    let Some(session) = session else {
        span.record("status", "anonymous");
        return (
            StatusCode::UNAUTHORIZED,
            Json(StatusBody::plain("anonymous")),
        )
            .into_response();
    };

    span.record("status", "ok");
    (
        StatusCode::OK,
        Json(WhoamiBody {
            identity: WhoamiIdentity {
                public_id: session.identity_public_id.to_string_lossy(),
                display_name: session.identity_display_name,
            },
            account: WhoamiAccount {
                public_id: session.account_public_id.to_string_lossy(),
                name: session.account_name,
                kind: session.account_kind.as_str(),
            },
            capabilities: session.capabilities.iter().map(|c| c.as_str()).collect(),
            expires_at: session.expires_at,
        }),
    )
        .into_response()
}

/// Name of the cookie these routes set, re-exported for tests and the shell.
pub const COOKIE_NAME: &str = SESSION_COOKIE;
