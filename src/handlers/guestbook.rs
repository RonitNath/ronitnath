//! Guestbook: the demo vertical slice (table → store → JSON API → island).

use askama::Template;
use axum::Json;
use axum::extract::State;
use axum::response::Response;

use crate::error::AppError;
use crate::state::AppState;
use crate::store::guestbook::{GuestbookEntry, NewGuestbookEntry};
use crate::view::render;

const MAX_FIELD_LEN: usize = 500;

#[derive(Template)]
#[template(path = "guestbook.html")]
struct GuestbookTemplate {
    nav_active: &'static str,
}

/// Serves the guestbook page; the entry list and form are a Solid island
/// hydrated client-side (see `ts/src/islands/Guestbook.tsx`).
pub async fn index(State(_state): State<AppState>) -> Result<Response, AppError> {
    render(GuestbookTemplate {
        nav_active: "guestbook",
    })
}

#[utoipa::path(
    get,
    path = "/api/guestbook",
    tag = "guestbook",
    responses((status = 200, description = "List all guestbook entries", body = [GuestbookEntry]))
)]
pub async fn api_list(State(state): State<AppState>) -> Result<Json<Vec<GuestbookEntry>>, AppError> {
    Ok(Json(state.store().list_guestbook().await?))
}

#[utoipa::path(
    post,
    path = "/api/guestbook",
    tag = "guestbook",
    request_body = NewGuestbookEntry,
    responses(
        (status = 200, description = "Entry created", body = GuestbookEntry),
        (status = 422, description = "Author or message was empty or too long"),
    )
)]
pub async fn api_create(
    State(state): State<AppState>,
    Json(entry): Json<NewGuestbookEntry>,
) -> Result<Json<GuestbookEntry>, AppError> {
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

    Ok(Json(state.store().add_guestbook_entry(entry).await?))
}
