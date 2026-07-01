//! Router assembly and server bootstrap.

use axum::{Router, routing::get};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::info;

use crate::config::Config;
use crate::handlers;
use crate::state::AppState;

/// Builds the application router.
///
/// Keep this a readable table of contents: declare routes here and put the
/// request logic in [`crate::handlers`].
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(handlers::home::index))
        .route("/about", get(handlers::about::index))
        .nest_service("/static", ServeDir::new("static"))
        .fallback(handlers::errors::not_found)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Builds the app and serves it until the process is terminated.
pub async fn run() {
    let config = Config::from_env();
    let state = AppState::new();
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {}: {e}", config.bind_addr));
    info!("listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}
