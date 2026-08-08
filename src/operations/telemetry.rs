//! Tracing subscriber + HTTP `TraceLayer` (axum tracing-aka-logging example).

use std::time::Duration;

use axum::{
    Router,
    body::Body,
    extract::MatchedPath,
    http::Request,
    response::Response,
};
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{Span, info_span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Install the process-wide subscriber. Safe to call once from `main`.
pub fn init() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Match axum's example defaults: app + tower_http + extractor rejections.
                format!(
                    "{}=debug,tower_http=debug,axum::rejection=trace",
                    env!("CARGO_CRATE_NAME")
                )
                .into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Attach HTTP request tracing with end-to-end latency on every response.
pub fn layer_http_trace(router: Router) -> Router {
    router.layer(
        TraceLayer::new_for_http()
            .make_span_with(|request: &Request<Body>| {
                let matched_path = request
                    .extensions()
                    .get::<MatchedPath>()
                    .map(MatchedPath::as_str);

                info_span!(
                    "http_request",
                    method = %request.method(),
                    uri = %request.uri(),
                    matched_path,
                    latency_ms = tracing::field::Empty,
                    status = tracing::field::Empty,
                )
            })
            .on_request(|_request: &Request<Body>, _span: &Span| {
                tracing::debug!("request started");
            })
            .on_response(|response: &Response, latency: Duration, span: &Span| {
                let latency_ms = latency.as_secs_f64() * 1000.0;
                span.record("latency_ms", latency_ms);
                span.record("status", response.status().as_u16());
                tracing::info!(
                    latency_ms,
                    status = response.status().as_u16(),
                    "request completed"
                );
            })
            .on_failure(
                |error: ServerErrorsFailureClass, latency: Duration, _span: &Span| {
                    tracing::error!(
                        ?error,
                        latency_ms = latency.as_secs_f64() * 1000.0,
                        "request failed"
                    );
                },
            ),
    )
}
