//! Template-rendering helpers shared by handlers.

use askama::Template;
use axum::response::{Html, IntoResponse, Response};

use crate::error::AppError;

/// Exact release identity. OCI images inject this at runtime so a UI-only
/// revision does not invalidate the Rust compilation layer.
pub fn release_revision() -> &'static str {
    static REVISION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    REVISION
        .get_or_init(|| {
            std::env::var("RELEASE_REVISION").unwrap_or_else(|_| env!("GIT_HASH").to_string())
        })
        .as_str()
}

/// Renders an Askama template into an HTML response.
///
/// A render failure becomes [`AppError::Render`], so handlers can use `?`.
pub fn render<T: Template>(template: T) -> Result<Response, AppError> {
    // Static entrypoints use a release-specific query parameter. This keeps
    // Cloudflare's long-lived `/static/*` cache from serving a prior island
    // bundle after an atomic release flip without putting a dynamic field on
    // every Askama template context.
    let html = template
        .render()?
        .replace("__ASSET_VERSION__", release_revision());
    Ok(Html(html).into_response())
}
