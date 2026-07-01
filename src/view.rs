//! Template-rendering helpers shared by handlers.

use askama::Template;
use axum::response::{Html, IntoResponse, Response};

use crate::error::AppError;

/// Renders an Askama template into an HTML response.
///
/// A render failure becomes [`AppError::Render`], so handlers can use `?`.
pub fn render<T: Template>(template: T) -> Result<Response, AppError> {
    Ok(Html(template.render()?).into_response())
}
