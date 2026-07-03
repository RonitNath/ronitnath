//! Guestbook: the demo vertical slice (table → store → JSON API → island),
//! and the account-scoping exemplar — every entry belongs to an account,
//! and every route requires an [`AccountScope`] rather than trusting a raw
//! id from the request. See `src/store/guestbook.rs` for the scoped
//! queries themselves.

use askama::Template;
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;

use crate::auth::AccountScope;
use crate::auth::csrf;
use crate::auth::extract::NavUser;
use crate::error::AppError;
use crate::state::AppState;
use crate::store::guestbook::{GuestbookEntry, NewGuestbookEntry};
use crate::view::render;

const MAX_FIELD_LEN: usize = 500;

#[derive(Template)]
#[template(path = "guestbook.html")]
struct GuestbookTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    account_name: String,
}

/// Serves the guestbook page; the entry list and form are a Solid island
/// hydrated client-side (see `ts/src/islands/Guestbook.tsx`). Requires
/// login — [`AccountScope`]'s extractor rejection redirects to `/login`
/// for HTML requests, so an unauthenticated visitor never sees this.
pub async fn index(scope: AccountScope) -> Result<Response, AppError> {
    render(GuestbookTemplate {
        nav_active: "guestbook",
        current_user: Some(NavUser {
            display_name: scope.display_name,
            csrf_token: scope.csrf_token.unwrap_or_default(),
        }),
        account_name: scope.account_name,
    })
}

#[utoipa::path(
    get,
    path = "/api/guestbook",
    tag = "guestbook",
    responses((status = 200, description = "List this account's guestbook entries", body = [GuestbookEntry]))
)]
pub async fn api_list(
    State(state): State<AppState>,
    scope: AccountScope,
) -> Result<Json<Vec<GuestbookEntry>>, AppError> {
    Ok(Json(state.store().list_guestbook(scope.account_id).await?))
}

#[utoipa::path(
    post,
    path = "/api/guestbook",
    tag = "guestbook",
    request_body = NewGuestbookEntry,
    responses(
        (status = 200, description = "Entry created", body = GuestbookEntry),
        (status = 422, description = "Author or message was empty or too long"),
        (status = 403, description = "Missing/invalid X-CSRF-Token header"),
    )
)]
pub async fn api_create(
    State(state): State<AppState>,
    scope: AccountScope,
    headers: HeaderMap,
    Json(entry): Json<NewGuestbookEntry>,
) -> Result<Json<GuestbookEntry>, AppError> {
    let submitted = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    csrf::verify(&scope, submitted)?;

    if entry.author.trim().is_empty() || entry.message.trim().is_empty() {
        return Err(AppError::Invalid(
            "author and message must not be empty".into(),
        ));
    }
    if entry.author.len() > MAX_FIELD_LEN || entry.message.len() > MAX_FIELD_LEN {
        return Err(AppError::Invalid(format!(
            "author and message must be under {MAX_FIELD_LEN} characters"
        )));
    }

    Ok(Json(state.store().add_guestbook_entry(scope.account_id, entry).await?))
}
