//! Router assembly and server bootstrap.

use axum::middleware;
use axum::routing::get;
use axum::{Json, Router};
use tower::ServiceBuilder;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::config::Config;
use crate::error;
use crate::handlers;
use crate::openapi::ApiDoc;
use crate::state::AppState;
use crate::store::Store;
use crate::telemetry;

/// Builds the application router.
///
/// Keep this a readable table of contents: declare routes here and put the
/// request logic in [`crate::handlers`]. Page routes use the plain `.route()`
/// pass-through; JSON API routes go through `.routes(routes!(...))` so they
/// show up in `/api/openapi.json` too.
pub fn build_router(state: AppState) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .route("/", get(handlers::home::index))
        .route("/about", get(handlers::about::index))
        .route("/guestbook", get(handlers::guestbook::index))
        .routes(routes!(
            handlers::guestbook::api_list,
            handlers::guestbook::api_create
        ))
        .routes(routes!(handlers::health::healthz))
        .routes(routes!(handlers::client_errors::report))
        .nest_service("/static", ServeDir::new("static"))
        .fallback(handlers::errors::not_found)
        .layer(
            ServiceBuilder::new()
                // Assign (or trust an inbound) x-request-id before anything
                // else runs, so every later layer can rely on it.
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(telemetry::make_span)
                        .on_response(telemetry::on_response),
                )
                // Renders the templated page for any AppError response.
                .layer(middleware::from_fn(error::render_error_pages))
                .layer(PropagateRequestIdLayer::x_request_id()),
        )
        .with_state(state)
        .split_for_parts();

    router.route(
        "/api/openapi.json",
        get(move || {
            let api = api.clone();
            async move { Json(api) }
        }),
    )
}

/// Builds the app and serves it until the process is terminated.
pub async fn run() {
    let config = Config::from_env();
    let store = Store::connect(&config.database_url)
        .await
        .unwrap_or_else(|e| panic!("failed to connect to database at {}: {e}", config.database_url));
    let state = AppState::new(store);
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {}: {e}", config.bind_addr));
    info!("listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}
