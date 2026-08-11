//! The `/manage` data browser: every durable data model, viewable in one place.
//!
//! Server-rendered behind the `manage` capability guard (wired in
//! [`crate::auth::routes`]). `/manage` lists the models with live row counts;
//! `/manage/{model}` shows the latest rows of one model.
//!
//! Two invariants this module enforces at the rendering boundary:
//!
//! - **Integer ids never leave the server.** Every table is displayed through
//!   joins to `public_id`s, emails, and names; the join keys themselves are
//!   not in the HTML.
//! - **Secrets are redacted structurally.** A password row renders only its
//!   PHC algorithm name; a session row renders an 8-character digest prefix.
//!   The full values are never interpolated, so no template change can leak
//!   them by accident.

pub mod queries;
mod render;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use tracing::{instrument, warn};

use crate::auth::AuthState;
pub use queries::{DataModel, TableData};

/// `/manage` — the model index with row counts.
#[instrument(name = "manage.index", skip(state))]
pub async fn index(State(state): State<AuthState>) -> Response {
    let mut counts = Vec::with_capacity(DataModel::ALL.len());
    for model in DataModel::ALL {
        match queries::count(&state.db, *model).await {
            Ok(count) => counts.push((*model, count)),
            Err(err) => {
                warn!(%err, model = model.slug(), "count query failed");
                return unavailable();
            }
        }
    }
    Html(render::index_page(&counts)).into_response()
}

/// `/manage/{model}` — the latest rows of one model.
#[instrument(name = "manage.model", skip(state))]
pub async fn model_page(State(state): State<AuthState>, Path(slug): Path<String>) -> Response {
    let Some(model) = DataModel::parse(&slug) else {
        return (StatusCode::NOT_FOUND, Html(render::not_found_page(&slug))).into_response();
    };

    let total = match queries::count(&state.db, model).await {
        Ok(total) => total,
        Err(err) => {
            warn!(%err, model = model.slug(), "count query failed");
            return unavailable();
        }
    };
    match queries::rows(&state.db, model).await {
        Ok(table) => Html(render::model_page(model, total, &table)).into_response(),
        Err(err) => {
            warn!(%err, model = model.slug(), "row query failed");
            unavailable()
        }
    }
}

/// A database fault is ours, not the caller's: 503, never an empty table.
fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Html(render::unavailable_page()),
    )
        .into_response()
}
