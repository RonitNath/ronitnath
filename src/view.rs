//! Template-rendering helpers shared by handlers.

use askama::Template;
use axum::response::{Html, IntoResponse, Response};

use crate::error::AppError;

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
        .replace("__ASSET_VERSION__", env!("GIT_HASH"));
    Ok(Html(html).into_response())
}
