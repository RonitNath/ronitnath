use askama::Template;
use axum::response::{Html, IntoResponse, Response};

use crate::AppState;

pub(crate) fn render_home(state: &AppState) -> Response {
    let template = HomeTemplate {
        script_src: state.manifest.entry("site.ts"),
        css_files: state.manifest.css_for_entry("site.ts"),
    };
    html(template.render())
}

pub(crate) fn render_events(state: &AppState) -> Response {
    let template = EventsTemplate {
        script_src: state.manifest.entry("site.ts"),
        css_files: state.manifest.css_for_entry("site.ts"),
    };
    html(template.render())
}

fn html(result: Result<String, askama::Error>) -> Response {
    match result {
        Ok(body) => Html(body).into_response(),
        Err(err) => {
            tracing::error!(?err, "template render failed");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "template error",
            )
                .into_response()
        }
    }
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    script_src: Option<String>,
    css_files: Vec<String>,
}

#[derive(Template)]
#[template(path = "events.html")]
struct EventsTemplate {
    script_src: Option<String>,
    css_files: Vec<String>,
}
