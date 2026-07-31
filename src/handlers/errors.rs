//! Fallback and error responses.

use crate::error::AppError;

/// Fallback handler for routes that did not match. Returns [`AppError::NotFound`],
/// which renders the templated 404 page.
pub async fn not_found() -> AppError {
    AppError::NotFound
}
