use axum::{Router, extract::State, response::Response, routing::get};

use crate::{AppState, render};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/events", get(events))
        .route("/healthz", get(healthz))
}

async fn home(State(state): State<AppState>) -> Response {
    render::render_home(&state)
}

async fn events(State(state): State<AppState>) -> Response {
    render::render_events(&state)
}

async fn healthz() -> &'static str {
    "ok"
}
