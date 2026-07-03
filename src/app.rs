//! Router assembly and server bootstrap.

use std::net::SocketAddr;
use std::time::Duration;

use axum::http::StatusCode;
use axum::middleware;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::ServeDir;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::config::Config;
use crate::error;
use crate::handlers;
use crate::openapi::ApiDoc;
use crate::rate_limit::{self, RateLimiter};
use crate::security_headers;
use crate::state::AppState;
use crate::store::Store;
use crate::telemetry;

/// Builds the application router.
///
/// Keep this a readable table of contents: declare routes here and put the
/// request logic in [`crate::handlers`]. Page routes use the plain `.route()`
/// pass-through; JSON API routes go through `.routes(routes!(...))` so they
/// show up in `/api/openapi.json` too.
pub fn build_router(state: AppState, config: &Config) -> Router {
    // Unauthenticated write endpoints share one rate-limit budget — see
    // `rate_limit.rs`. Built as separate fragments and merged in below so
    // `route_layer` (which wraps everything registered so far in the
    // fragment it's called on) only ever touches these two routes, not the
    // whole router.
    let rate_limiter = RateLimiter::new(config.rate_limit_per_minute, config.trust_proxy);
    let guestbook_api = OpenApiRouter::new()
        .routes(routes!(
            handlers::guestbook::api_list,
            handlers::guestbook::api_create
        ))
        .route_layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limit::enforce,
        ));
    let client_error_api = OpenApiRouter::new()
        .routes(routes!(handlers::client_errors::report))
        .route_layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limit::enforce,
        ));

    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .route("/", get(handlers::home::index))
        .route("/about", get(handlers::about::index))
        .route("/guestbook", get(handlers::guestbook::index))
        .merge(guestbook_api)
        .routes(routes!(handlers::health::healthz))
        .merge(client_error_api)
        .nest_service("/static", ServeDir::new("static"))
        .fallback(handlers::errors::not_found)
        // Layered individually (not via one `ServiceBuilder`): bundling the
        // `from_fn` error-page middleware together with `tower_http`'s
        // generic body-rewrapping layers (timeout, body-limit) in a single
        // `ServiceBuilder` stack defeats rustc's inference before it ever
        // reaches a concrete `Router` to anchor against. Each `.layer()`
        // call here resolves against the router directly instead.
        //
        // Router::layer's *last* call becomes the *outermost* wrapper (the
        // opposite of `ServiceBuilder`, where the first call is outermost)
        // — so this list reads innermost-first. Outer-to-inner, a request
        // actually passes through: SetRequestId, Propagate, the security
        // headers, Trace, the error-page middleware, Timeout, then the
        // body-size limit, then the route.
        //
        // Timeout and body-limit sit innermost because both rewrap the
        // request/response body into types `Trace`'s `make_span` and the
        // error-page middleware's `Request` extractor aren't generic over
        // — those two are pinned to the plain `Body` type.
        .layer(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(config.request_timeout_secs),
        ))
        // Renders the templated page for any AppError response.
        .layer(middleware::from_fn(error::render_error_pages))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(telemetry::make_span)
                .on_response(telemetry::on_response),
        )
        .layer(security_headers::content_security_policy())
        .layer(security_headers::referrer_policy())
        .layer(security_headers::x_frame_options())
        .layer(security_headers::x_content_type_options())
        // Propagate + the security headers wrap everything below, including
        // the timeout layer: if a request times out, the layers *inside*
        // the timeout (trace, error pages, the handler) never run for it,
        // so anything that must land on every response has to sit out here.
        .layer(PropagateRequestIdLayer::x_request_id())
        // Assign (or trust an inbound) x-request-id before anything else
        // runs, so every later layer can rely on it.
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
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

/// Waits for Ctrl+C or SIGTERM so [`run`] can drain in-flight requests
/// before the process exits, instead of dropping connections on deploy.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutting down");
}

/// Builds the app and serves it until the process is terminated.
pub async fn run() {
    let config = Config::from_env();
    let store = Store::connect(&config.database_url)
        .await
        .unwrap_or_else(|e| panic!("failed to connect to database at {}: {e}", config.database_url));
    let state = AppState::new(store);
    let app = build_router(state, &config);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {}: {e}", config.bind_addr));
    info!("listening on http://{}", listener.local_addr().unwrap());

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .unwrap();
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use axum::http::{StatusCode, header};
    use base64::Engine;
    use sha2::{Digest, Sha256};

    use crate::test_util::{get, post_bytes, post_json, post_json_from, test_app};

    #[tokio::test]
    async fn home_page_renders() {
        let app = test_app().await;
        let (status, headers, body) = get(&app, "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            headers
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("stage_1"));
    }

    #[tokio::test]
    async fn unknown_route_renders_templated_404_with_request_id() {
        let app = test_app().await;
        let (status, headers, body) = get(&app, "/nonexistent").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let request_id = headers
            .get("x-request-id")
            .expect("x-request-id header on every response")
            .to_str()
            .unwrap()
            .to_string();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains(&format!("ref: {request_id}")));
    }

    #[tokio::test]
    async fn guestbook_roundtrips_through_router_and_store() {
        let app = test_app().await;
        let (status, _, body) = post_json(
            &app,
            "/api/guestbook",
            &serde_json::json!({"author": "test", "message": "hello"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(created["author"], "test");

        let (status, _, body) = get(&app, "/api/guestbook").await;
        assert_eq!(status, StatusCode::OK);
        let entries: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        // The seed migration inserts one entry; this test adds a second.
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn guestbook_rejects_empty_fields_with_standard_error_shape() {
        let app = test_app().await;
        let (status, _, body) = post_json(
            &app,
            "/api/guestbook",
            &serde_json::json!({"author": "", "message": "hello"}),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(error["error"].is_string());
        assert!(error["request_id"].is_string());
    }

    #[tokio::test]
    async fn healthz_reports_version_and_uptime() {
        let app = test_app().await;
        let (status, _, body) = get(&app, "/healthz").await;
        assert_eq!(status, StatusCode::OK);
        let health: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(health["version"].is_string());
        assert!(health["uptime_secs"].is_number());
    }

    #[tokio::test]
    async fn oversized_body_is_rejected_with_413() {
        let app = test_app().await;
        // Config::for_tests() caps bodies at 1024 bytes.
        let big_message = "x".repeat(2048);
        let (status, _, _) = post_bytes(
            &app,
            "/api/guestbook",
            "application/json",
            serde_json::to_vec(&serde_json::json!({"author": "a", "message": big_message}))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn responses_carry_security_headers_and_csp_matches_served_theme_script() {
        let app = test_app().await;
        let (_, headers, body) = get(&app, "/").await;

        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
        let csp = headers
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();

        // Drift guard: hash the <script> the router actually served and
        // confirm it's the same hash the CSP header allows. The header
        // value is derived from the same template file
        // (`security_headers::inline_tag_hash`), so this can't drift from
        // a code change — but re-deriving it from the served bytes also
        // catches Askama-rendering surprises a purely static check
        // wouldn't (e.g. escaping differences).
        let body = String::from_utf8(body.to_vec()).unwrap();
        let start = body.find("<script>").unwrap() + "<script>".len();
        let end = start + body[start..].find("</script>").unwrap();
        let served_script = &body[start..end];
        let digest = Sha256::digest(served_script.as_bytes());
        let hash = base64::engine::general_purpose::STANDARD.encode(digest);
        assert!(
            csp.contains(&hash),
            "CSP {csp:?} does not allow the served inline script hash {hash}"
        );
    }

    #[tokio::test]
    async fn rate_limiter_returns_429_after_budget_exhausted() {
        let app = test_app().await;
        let addr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));
        let body = serde_json::json!({"message": "boom", "source": "test", "line": 1, "col": 1, "stack": ""});

        // Config::for_tests() allows 10/min; the 11th from the same client
        // should be refused.
        for _ in 0..10 {
            let (status, _, _) = post_json_from(&app, "/api/client-errors", &body, addr).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }
        let (status, _, _) = post_json_from(&app, "/api/client-errors", &body, addr).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    }
}
