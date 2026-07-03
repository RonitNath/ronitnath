//! Baseline security response headers, applied to every response.
//!
//! Registered in [`crate::app::build_router`] as the outermost layers (aside
//! from request-id assignment), so they land on error and timeout responses
//! too, not just successful handler output.

use std::sync::LazyLock;

use axum::http::{HeaderName, HeaderValue};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest, Sha256};
use tower_http::set_header::SetResponseHeaderLayer;

/// `_theme.html`'s inline `<style>`/`<script>` blocks (see AGENTS.md — the
/// FOUC-prevention block is deliberately duplicated with `base.css`) are
/// read here at compile time rather than re-typed, so the CSP hash below
/// can never drift from what's actually served.
const THEME_TEMPLATE: &str = include_str!("../templates/_theme.html");

fn inline_tag_hash(tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = THEME_TEMPLATE
        .find(&open)
        .unwrap_or_else(|| panic!("_theme.html has no inline <{tag}>"))
        + open.len();
    let end = THEME_TEMPLATE[start..]
        .find(&close)
        .unwrap_or_else(|| panic!("_theme.html's <{tag}> is never closed"))
        + start;
    let body = &THEME_TEMPLATE[start..end];
    format!("'sha256-{}'", BASE64.encode(Sha256::digest(body.as_bytes())))
}

/// The CSP value, computed once. `pub(crate)` so router tests can assert it
/// still matches the rendered page rather than trusting it blindly.
pub(crate) static CONTENT_SECURITY_POLICY: LazyLock<HeaderValue> = LazyLock::new(|| {
    let policy = format!(
        "default-src 'self'; \
         script-src 'self' {}; \
         style-src 'self' {}; \
         img-src 'self' data:; \
         connect-src 'self'; \
         frame-ancestors 'none'; \
         base-uri 'self'; \
         form-action 'self'",
        inline_tag_hash("script"),
        inline_tag_hash("style"),
    );
    HeaderValue::from_str(&policy).expect("CSP policy is valid header value")
});

pub fn content_security_policy() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("content-security-policy"),
        CONTENT_SECURITY_POLICY.clone(),
    )
}

pub fn x_content_type_options() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    )
}

pub fn x_frame_options() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    )
}

pub fn referrer_policy() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    )
}
