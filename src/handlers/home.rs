//! Home page.

use askama::Template;
use axum::{extract::State, response::Response};

use crate::error::AppError;
use crate::state::AppState;
use crate::view::render;

/// The home page template.
#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    nav_active: &'static str,
}

/// Serves the home page.
pub async fn index(State(_state): State<AppState>) -> Result<Response, AppError> {
    render(HomeTemplate { nav_active: "home" })
}
