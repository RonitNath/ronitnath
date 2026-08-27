//! Browser-freshness headers, applied to every response in one place.
//!
//! Two rules, and the split between them is the whole point:
//!
//! * **Documents and API responses are `no-store`.** They carry session-shaped
//!   state; a back-button replay out of the bfcache is a stale-permissions bug.
//! * **Assets are `no-cache, must-revalidate`.** Their URLs are stable across
//!   releases — there is no `?v=` cache-busting anywhere — so the browser must
//!   ask every time and gets a 304 for its trouble.
//!
//! `X-RN-App-Version` rides on everything, which is what makes "which revision
//! is nyc serving" answerable with one `curl -I` against any URL at all.

use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::state::AppState;

pub const VERSION_HEADER: HeaderName = HeaderName::from_static("x-rn-app-version");

/// The `Cache-Control` value for a path. Asset prefixes are listed rather than
/// inferred from an extension: a route is an asset because we serve it as one,
/// and a `.json` API response must not become cacheable by renaming.
#[must_use]
pub fn cache_policy(path: &str) -> &'static str {
    const ASSETS: &[&str] = &[
        "/static/",
        "/pkg/",
        "/app/pkg/",
        "/org/pkg/",
        "/platform/pkg/",
        "/favicon.ico",
    ];
    if ASSETS.iter().any(|prefix| path.starts_with(prefix)) {
        "no-cache, must-revalidate"
    } else {
        "no-store"
    }
}

pub async fn freshness(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_policy(&path)),
    );
    headers.insert(
        VERSION_HEADER,
        HeaderValue::from_str(&state.version)
            .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::cache_policy;

    #[test]
    fn assets_revalidate_and_everything_else_refuses_to_be_stored() {
        for asset in [
            "/static/stars/bright.bin",
            "/pkg/starscape/rn-starscape.js",
            "/app/pkg/rn-app_bg.wasm",
            "/platform/pkg/rn-platform.js",
            "/favicon.ico",
        ] {
            assert_eq!(cache_policy(asset), "no-cache, must-revalidate", "{asset}");
        }
        for document in ["/", "/healthz", "/readyz", "/version", "/api/whoami"] {
            assert_eq!(cache_policy(document), "no-store", "{document}");
        }
    }
}
