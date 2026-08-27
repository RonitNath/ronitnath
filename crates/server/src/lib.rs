//! `rn-site` — the server: ops routes, static assets and the public presence.
//!
//! Composition lives in [`router`]; every feature module owns its own handlers,
//! its templates and its tests. `/api/*`, the auth pages and the tier shells
//! are S2's and are deliberately absent rather than stubbed — a route that
//! exists and does nothing is indistinguishable from a broken one.

pub mod assets;
pub mod config;
pub mod db;
pub mod http;
pub mod ops;
pub mod presence;
pub mod shutdown;
pub mod state;
pub mod telemetry;

use axum::Router;

pub use state::AppState;

/// The whole HTTP surface. Feature routers merge as peers; the freshness layer
/// wraps all of them so no route can be added later that forgets the version
/// header or picks its own cache policy.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(ops::router())
        .merge(presence::router())
        .merge(assets::router())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            http::freshness,
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}
