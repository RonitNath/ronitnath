//! About page.

use askama::Template;
use axum::response::Response;

use crate::auth::extract::{NavContext, NavUser};
use crate::error::AppError;
use crate::view::render;

/// The about page template.
#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
}

/// Serves the about page.
pub async fn index(NavContext(current_user): NavContext) -> Result<Response, AppError> {
    render(AboutTemplate {
        nav_active: "about",
        current_user,
    })
}
