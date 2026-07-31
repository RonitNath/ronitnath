//! [`AccountScope`] — the extractor handlers use to get an authenticated
//! identity + its active account + role, instead of trusting a raw id from
//! the request. Also [`NavContext`], the lightweight "is anyone logged in"
//! read every page (even public ones) uses to render the nav.

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{Extensions, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use serde_json::json;

use crate::auth::Role;
use crate::state::AppState;
use crate::store::sessions::SessionContext;

/// What a page needs to render its nav — nothing more. Cheap to build on
/// every request (even ones that end up unauthenticated), unlike
/// [`AccountScope`], which never fails softly.
#[derive(Debug, Clone)]
pub struct NavUser {
    pub display_name: String,
    pub csrf_token: String,
    pub is_guest: bool,
}

/// Reads the session context [`crate::auth::middleware::attach_session`]
/// already resolved for this request. Used both by the [`NavContext`]
/// extractor and directly by `error::render_error_pages`, which works with
/// a raw [`Request`](axum::extract::Request) rather than an extractor.
pub fn nav_user_from_extensions(extensions: &Extensions) -> Option<NavUser> {
    extensions
        .get::<Option<SessionContext>>()
        .cloned()
        .flatten()
        .map(|ctx| NavUser {
            display_name: ctx.display_name,
            csrf_token: ctx.csrf_token,
            is_guest: ctx.account_purpose == "guest",
        })
}

pub struct NavContext(pub Option<NavUser>);

impl<S> FromRequestParts<S> for NavContext
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(NavContext(nav_user_from_extensions(&parts.extensions)))
    }
}

/// An authenticated identity, its active account, and its role there —
/// re-derived from a live `memberships` row on every request (via
/// [`crate::auth::middleware::attach_session`] for cookie sessions, or a
/// direct lookup here for bearer tokens), so a revoked session or
/// membership takes effect on the very next request.
///
/// `csrf_token` is `None` for bearer-token auth: token auth requires
/// reading an `Authorization` header, which a cross-site request can't do,
/// so it's CSRF-immune by construction — see [`crate::auth::csrf`].
pub struct AccountScope {
    pub identity_id: i64,
    pub account_id: i64,
    pub role: Role,
    pub display_name: String,
    pub account_name: String,
    pub session_id: Option<i64>,
    pub csrf_token: Option<String>,
}

impl AccountScope {
    /// 403s unless the role on this account meets `min`.
    pub fn require(&self, min: Role) -> Result<(), crate::error::AppError> {
        if self.role >= min {
            Ok(())
        } else {
            Err(crate::error::AppError::Forbidden(
                "you don't have permission to do that".into(),
            ))
        }
    }
}

impl FromRequestParts<AppState> for AccountScope {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(token) = bearer_token(&parts.headers) {
            return match crate::auth::api_token::verify(state.store(), token).await {
                Ok(Some(verified)) => Ok(AccountScope {
                    identity_id: verified.identity_id,
                    account_id: verified.account_id,
                    role: Role::parse(&verified.role),
                    display_name: verified.display_name,
                    account_name: verified.account_name,
                    session_id: None,
                    csrf_token: None,
                }),
                _ => Err(unauthenticated(parts)),
            };
        }

        match nav_user_from_extensions(&parts.extensions).is_some() {
            true => {
                let ctx = parts
                    .extensions
                    .get::<Option<SessionContext>>()
                    .cloned()
                    .flatten()
                    .expect("checked Some above");
                Ok(AccountScope {
                    identity_id: ctx.identity_id,
                    account_id: ctx.account_id,
                    role: Role::parse(&ctx.role),
                    display_name: ctx.display_name,
                    account_name: ctx.account_name,
                    session_id: Some(ctx.session_id),
                    csrf_token: Some(ctx.csrf_token),
                })
            }
            false => Err(unauthenticated(parts)),
        }
    }
}

/// A claimed guest session, resolved back to the owner's account and person.
/// The session's own guest account is deliberately not exposed to domain handlers.
pub struct GuestScope {
    pub identity_id: i64,
    pub owner_account_id: i64,
    pub person_id: i64,
    pub person_name: String,
    pub session_id: i64,
    pub csrf_token: String,
}

impl FromRequestParts<AppState> for GuestScope {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(ctx) = parts
            .extensions
            .get::<Option<SessionContext>>()
            .cloned()
            .flatten()
        else {
            return Err(unauthenticated(parts));
        };
        let binding = state
            .store()
            .active_guest_binding(ctx.identity_id)
            .await
            .ok()
            .flatten()
            .filter(|binding| Some(binding.owner_account_id) == state.owner_account_id());
        match binding {
            Some(binding) => Ok(Self {
                identity_id: ctx.identity_id,
                owner_account_id: binding.owner_account_id,
                person_id: binding.person_id,
                person_name: binding.person_name,
                session_id: ctx.session_id,
                csrf_token: ctx.csrf_token,
            }),
            None => Err(unauthenticated(parts)),
        }
    }
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

fn unauthenticated(parts: &Parts) -> Response {
    if parts.uri.path().starts_with("/api/") {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "authentication required"})),
        )
            .into_response()
    } else {
        Redirect::to(&format!("/login?next={}", parts.uri.path())).into_response()
    }
}
