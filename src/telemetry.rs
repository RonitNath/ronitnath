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
                .unwrap_or_else(|_| "ronitnath=debug,tower_http=info".into()),
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

    let path = sanitize_path(request.uri().path());
    tracing::info_span!(
        "request",
        method = %request.method(),
        path = %path,
        request_id,
    )
}

/// Redacts capability credentials while preserving route shape for diagnostics.
/// This only changes telemetry; axum continues routing on the original URI.
fn sanitize_path(path: &str) -> String {
    for (prefix, placeholder) in [("/e/", "{token}"), ("/api/e/", "{token}")] {
        if let Some(rest) = path.strip_prefix(prefix) {
            let suffix = rest.find('/').map_or("", |at| &rest[at..]);
            return format!("{prefix}{placeholder}{suffix}");
        }
    }
    if let Some(feed) = path.strip_prefix("/calendar/")
        && !feed.contains('/')
        && feed.ends_with(".ics")
    {
        return "/calendar/{feed}.ics".into();
    }
    if let Some(event_ref) = path.strip_prefix("/events/")
        && let Some(token) = event_ref.strip_suffix("/ics")
        && !token.contains('/')
    {
        return "/events/{token}/ics".into();
    }
    path.to_owned()
}

/// Records the outcome of a request against its [`make_span`] span.
pub fn on_response<B>(response: &Response<B>, latency: Duration, _span: &Span) {
    tracing::info!(
        status = response.status().as_u16(),
        latency_ms = latency.as_millis() as u64,
        "response"
    );
}

#[cfg(test)]
mod tests {
    use super::sanitize_path;

    #[test]
    fn capability_paths_are_redacted_without_losing_route_shape() {
        let sentinel = "SENTINEL-CAPABILITY-CREDENTIAL";
        for (raw, expected) in [
            (format!("/e/{sentinel}"), "/e/{token}"),
            (format!("/e/{sentinel}/claim"), "/e/{token}/claim"),
            (
                format!("/e/{sentinel}/photos/42/medium"),
                "/e/{token}/photos/42/medium",
            ),
            (format!("/api/e/{sentinel}/rsvp"), "/api/e/{token}/rsvp"),
            (format!("/calendar/{sentinel}.ics"), "/calendar/{feed}.ics"),
            (format!("/events/{sentinel}/ics"), "/events/{token}/ics"),
        ] {
            let sanitized = sanitize_path(&raw);
            assert_eq!(sanitized, expected);
            assert!(!sanitized.contains(sentinel));
        }
        assert_eq!(sanitize_path("/calendar"), "/calendar");
        assert_eq!(sanitize_path("/events/42"), "/events/42");
    }
}
