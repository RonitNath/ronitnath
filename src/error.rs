//! The crate-wide error type and its HTTP representation.

use askama::Template;
use axum::{
    Json,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{Html, IntoResponse, Response},
};
use serde::Serialize;
use tower_http::request_id::RequestId;

/// Error type returned by fallible handlers.
///
/// Handlers return `Result<T, AppError>`; the [`IntoResponse`] impl maps each
/// variant to a status code and, via [`render_error_pages`], a rendered error
/// page. `?` works directly on `askama::Error`, `sqlx::Error`, and
/// `anyhow::Error` thanks to the `#[from]` variants — reach for
/// [`AppError::Other`] (via `anyhow`) for one-off errors, and add a named
/// variant when callers need to match on a specific failure.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested resource does not exist (→ 404).
    #[error("not found")]
    NotFound,

    /// The request itself was malformed in some way the caller can fix (→ 422).
    #[error("{0}")]
    Invalid(String),

    /// A route-specific request body limit was exceeded (→ 413).
    #[error("request body is too large")]
    PayloadTooLarge,

    /// Authenticated, but not allowed to do this (role check or CSRF
    /// mismatch) (→ 403).
    #[error("{0}")]
    Forbidden(String),

    /// Login failed: unknown email or wrong password (→ 401). Kept
    /// distinct from `Invalid` (422) since "who are you" and "that
    /// request doesn't make sense" are different failure classes.
    #[error("{0}")]
    InvalidCredentials(String),

    /// A template failed to render (→ 500).
    #[error("template render failed")]
    Render(#[from] askama::Error),

    /// A database operation failed (→ 500).
    #[error("database error")]
    Db(#[from] sqlx::Error),

    /// Any other, unexpected error (→ 500).
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AppError {
    /// The HTTP status this error maps to.
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::InvalidCredentials(_) => StatusCode::UNAUTHORIZED,
            AppError::Render(_) | AppError::Db(_) | AppError::Other(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    /// A user-facing message safe to show on the error page.
    fn user_message(&self) -> String {
        match self {
            AppError::NotFound => "The page you were looking for doesn't exist.".to_string(),
            AppError::Invalid(message) => message.clone(),
            AppError::PayloadTooLarge => "The uploaded file is too large.".to_string(),
            AppError::Forbidden(message) => message.clone(),
            AppError::InvalidCredentials(message) => message.clone(),
            AppError::Render(_) | AppError::Db(_) | AppError::Other(_) => {
                "Something went wrong on our end.".to_string()
            }
        }
    }
}

/// Carries an [`AppError`]'s status and message through response extensions
/// so [`render_error_pages`] can render the templated page with request
/// context that [`IntoResponse`] alone can't see.
#[derive(Clone)]
struct ErrorMeta {
    status: StatusCode,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Server-side faults are worth logging; a 404 or bad request is routine.
        if status.is_server_error() {
            tracing::error!(error = ?self, "request failed");
        }

        let mut response = status.into_response();
        response.extensions_mut().insert(ErrorMeta {
            status,
            message: self.user_message(),
        });
        response
    }
}

/// The error page, shared by every error response.
#[derive(Template)]
#[template(path = "error.html")]
struct ErrorTemplate {
    nav_active: &'static str,
    status: u16,
    message: String,
    request_id: String,
    current_user: Option<crate::auth::extract::NavUser>,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    request_id: String,
}

/// Renders an [`ErrorMeta`]-tagged response (inserted by [`AppError`]'s
/// [`IntoResponse`] impl) as a page for normal requests, or JSON for
/// `/api/*` requests — tagging either with the request's id so a bug report
/// maps back to a log line.
pub async fn render_error_pages(request: Request, next: Next) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("-")
        .to_string();
    // Read before `next.run` consumes `request` — inserted by
    // `auth::middleware::attach_session`, which must sit outside (run
    // before) this middleware in the layer stack.
    let current_user = crate::auth::extract::nav_user_from_extensions(request.extensions());

    let response = next.run(request).await;
    let Some(meta) = response.extensions().get::<ErrorMeta>() else {
        return response;
    };

    if is_api {
        return (
            meta.status,
            Json(ErrorBody {
                error: meta.message.clone(),
                request_id,
            }),
        )
            .into_response();
    }

    let page = ErrorTemplate {
        nav_active: "",
        status: meta.status.as_u16(),
        message: meta.message.clone(),
        request_id,
        current_user,
    };

    match page.render() {
        Ok(html) => (meta.status, Html(html)).into_response(),
        // Fall back to plain text if even the error page won't render.
        Err(_) => (meta.status, meta.message.clone()).into_response(),
    }
}
