//! Home page.

use askama::Template;
use axum::response::Response;

use crate::auth::extract::{NavContext, NavUser};
use crate::error::AppError;
use crate::view::render;

/// The home page template.
#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
}

/// Serves the home page.
pub async fn index(NavContext(current_user): NavContext) -> Result<Response, AppError> {
    render(HomeTemplate {
        nav_active: "home",
        current_user,
    })
}
