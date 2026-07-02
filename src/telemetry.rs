//! Logging / tracing setup.

use std::time::Duration;

use axum::extract::Request;
use axum::http::Response;
use tower_http::request_id::RequestId;
use tracing::Span;

/// Initializes the global tracing subscriber, honoring `RUST_LOG` when set.
pub fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stage_1=debug,tower_http=info".into()),
        )
        .init();
}

/// Builds the per-request span used by [`tower_http::trace::TraceLayer`].
///
/// Carries the request id so every log line inside a request's lifetime can
/// be traced back to the id echoed on its error page (see [`crate::error`]).
pub fn make_span(request: &Request) -> Span {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("-")
        .to_string();

    tracing::info_span!(
        "request",
        method = %request.method(),
        path = %request.uri().path(),
        request_id,
    )
}

/// Records the outcome of a request against its [`make_span`] span.
pub fn on_response<B>(response: &Response<B>, latency: Duration, _span: &Span) {
    tracing::info!(
        status = response.status().as_u16(),
        latency_ms = latency.as_millis() as u64,
        "response"
    );
}
