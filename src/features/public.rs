use axum::{
    Router,
    extract::State,
    http::{HeaderValue, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use islands::{Block, Document, FrontendAssets, Island, Markup};

const PUBLIC_CACHE: &str = "public, max-age=60";

pub fn router(frontend_assets: FrontendAssets) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/calendar", get(calendar))
        .with_state(frontend_assets)
}

async fn home(State(assets): State<FrontendAssets>) -> Response {
    render(assets, "home")
}

async fn calendar(State(assets): State<FrontendAssets>) -> Response {
    render(assets, "calendar")
}

fn render(assets: FrontendAssets, page: &'static str) -> Response {
    let (title, description, fallback) = match page {
        "home" => (
            "Ronit Nath",
            "Ronit Nath — Founder of Isoastra.",
            include_str!("../content/home.html"),
        ),
        "calendar" => (
            "Calendar · Upcoming plans",
            "Ronit Nath's public calendar.",
            include_str!("../content/calendar.html"),
        ),
        _ => unreachable!("the public route list is closed"),
    };
    let head = Markup::from_trusted(concat!(
        "<link rel=\"canonical\" href=\"https://ronitnath.com/\">",
        "<link rel=\"stylesheet\" href=\"/static/css/base.css\">",
        "<link rel=\"stylesheet\" href=\"/static/css/atmosphere.css\">",
        "<link rel=\"stylesheet\" href=\"/static/css/layout.css\">",
        "<link rel=\"stylesheet\" href=\"/static/css/components.css\">",
        "<link rel=\"stylesheet\" href=\"/static/css/events.css\">"
    ));
    let island = Island::new("public-root", serde_json::json!({ "page": page }))
        .expect("the product island name is valid")
        .fallback_markup(Markup::from_trusted(fallback));
    let blocks = [Block::Island(island)];
    let mut response = Html(
        Document::new(crate::build_stamp::BUILD_STAMP, &assets, title)
            .description(description)
            .head(head)
            .blocks(&blocks)
            .render(),
    )
    .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PUBLIC_CACHE),
    );
    response
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::router;

    #[tokio::test]
    async fn public_route_allowlist_has_the_two_ported_pages() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("app.abc123.js"), "javascript").unwrap();
        std::fs::write(
            directory.path().join("manifest.json"),
            r#"{"app.js":"app.abc123.js"}"#,
        )
        .unwrap();
        let assets = islands::FrontendAssets::load(directory.path()).unwrap();
        let app = router(assets);

        for (path, expected_copy) in [
            ("/", "Founder of Isoastra"),
            ("/calendar", "Agenda — next 12 months"),
        ] {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK, "{path}");
            let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
                .await
                .unwrap();
            assert!(String::from_utf8_lossy(&body).contains(expected_copy));
        }

        let missing = app
            .oneshot(Request::get("/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(missing.status(), axum::http::StatusCode::NOT_FOUND);
    }
}
