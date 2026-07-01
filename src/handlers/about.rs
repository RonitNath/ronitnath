//! About page.

use askama::Template;
use axum::{extract::State, response::Response};

use crate::error::AppError;
use crate::state::AppState;
use crate::view::render;

/// The about page template.
#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate {
    nav_active: &'static str,
}

/// Serves the about page.
pub async fn index(State(_state): State<AppState>) -> Result<Response, AppError> {
    render(AboutTemplate { nav_active: "about" })
}
