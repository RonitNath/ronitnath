//! The crate-wide error type and its HTTP representation.

use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

/// Error type returned by fallible handlers.
///
/// Handlers return `Result<T, AppError>`; the [`IntoResponse`] impl maps each
/// variant to a status code and a rendered error page. `?` works directly on
/// `askama::Error` and `anyhow::Error` thanks to the `#[from]` variants — reach
/// for [`AppError::Other`] (via `anyhow`) for one-off errors, and add a named
/// variant when callers need to match on a specific failure.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested resource does not exist (→ 404).
    #[error("not found")]
    NotFound,

    /// A template failed to render (→ 500).
    #[error("template render failed")]
    Render(#[from] askama::Error),

    /// Any other, unexpected error (→ 500).
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AppError {
    /// The HTTP status this error maps to.
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Render(_) | AppError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// A user-facing message safe to show on the error page.
    fn user_message(&self) -> &'static str {
        match self {
            AppError::NotFound => "The page you were looking for doesn't exist.",
            AppError::Render(_) | AppError::Other(_) => "Something went wrong on our end.",
        }
    }
}

/// The error page, shared by every error response.
#[derive(Template)]
#[template(path = "error.html")]
struct ErrorTemplate {
    nav_active: &'static str,
    status: u16,
    message: &'static str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Server-side faults are worth logging; a 404 is routine.
        if status.is_server_error() {
            tracing::error!(error = ?self, "request failed");
        }

        let page = ErrorTemplate {
            nav_active: "",
            status: status.as_u16(),
            message: self.user_message(),
        };

        match page.render() {
            Ok(html) => (status, Html(html)).into_response(),
            // Fall back to plain text if even the error page won't render.
            Err(_) => (status, self.user_message()).into_response(),
        }
    }
}
