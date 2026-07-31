use std::error::Error;

use axum::Router;
use http_runtime::{
    RuntimeConfig, apply_runtime_layers, operator_system_router, public_system_router,
};
use telemetry::Metrics;

pub struct Application {
    public_router: Router,
    operator_router: Router,
}

impl Application {
    /// Returns product routes plus public liveness and read-health routes.
    pub fn public_router(&self) -> Router {
        self.public_router.clone()
    }

    /// Returns only routes intended for the dedicated operator listener.
    pub fn operator_router(&self) -> Router {
        self.operator_router.clone()
    }

    pub async fn shutdown(&self) {}
}

pub async fn build(
    config: &RuntimeConfig,
    metrics: Metrics,
    static_dist: &std::path::Path,
) -> Result<Application, Box<dyn Error + Send + Sync>> {
    let frontend_assets = islands::FrontendAssets::load(static_dist)?;
    let health = http_runtime::Health::default();
    let public_routes = Router::new()
        // The island manifest lives in `static/public`; the legacy visual
        // assets intentionally remain at `/static/css` and `/static/fonts`.
        // One public root keeps those URL contracts intact while preserving the
        // generated manifest's `/static/public/` base.
        .nest(
            "/static",
            http_runtime::static_assets(
                std::path::Path::new("static"),
                http_runtime::AssetVisibility::Public,
            ),
        )
        .merge(crate::features::public::router(frontend_assets.clone()))
        .merge(public_system_router(
            health.clone(),
            crate::build_stamp::BUILD_STAMP,
        ));
    let operator_routes = operator_system_router(health, metrics.clone());
    Ok(Application {
        public_router: apply_runtime_layers(public_routes, config, metrics.clone()),
        operator_router: apply_runtime_layers(operator_routes, config, metrics),
    })
}

#[cfg(test)]
mod tests {
    use super::build;
    use http_runtime::RuntimeConfig;
    use telemetry::Metrics;

    #[tokio::test]
    async fn profile_startup_matches_selected_capabilities() {
        let config = RuntimeConfig::from_lookup(|key| match key {
            "APP_ENV" => Some("development".into()),
            "PUBLIC_ORIGIN" => Some("http://127.0.0.1:3000".into()),
            _ => None,
        })
        .unwrap();
        let static_dist = tempfile::tempdir().unwrap();
        std::fs::write(static_dist.path().join("app.abc123.js"), "javascript").unwrap();
        std::fs::write(
            static_dist.path().join("manifest.json"),
            r#"{"app.js":"app.abc123.js"}"#,
        )
        .unwrap();
        let result = build(&config, Metrics::default(), static_dist.path()).await;
        assert!(result.is_ok());
    }

    /// The shell's asset links have to resolve against the same router that
    /// serves them. This is the only test that puts the two together, and the
    /// mount is easy to get wrong in a way nothing else notices: a `ServeDir`
    /// merged at the root sees the whole request path, so its root must be the
    /// directory *containing* `dist`, not `dist` itself.
    #[tokio::test]
    async fn the_router_serves_the_asset_the_shell_links_to() {
        use tower::ServiceExt;

        let config = RuntimeConfig::from_lookup(|key| match key {
            "APP_ENV" => Some("development".into()),
            "PUBLIC_ORIGIN" => Some("http://127.0.0.1:3000".into()),
            _ => None,
        })
        .unwrap();
        let application = build(
            &config,
            Metrics::default(),
            &std::path::Path::new("static").join(islands::AssetScope::PUBLIC),
        )
        .await
        .unwrap();

        let shell = application
            .public_router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(shell.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        let script = body
            .split_once("src=\"")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(url, _)| url.to_owned())
            .expect("the shell links a module script");
        assert!(script.starts_with("/static/public/"), "{script}");

        let asset = application
            .public_router()
            .oneshot(
                axum::http::Request::builder()
                    .uri(&script)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(asset.status(), axum::http::StatusCode::OK, "{script}");
        assert_eq!(
            asset
                .headers()
                .get(axum::http::header::CACHE_CONTROL)
                .unwrap(),
            "public, max-age=31536000, immutable"
        );
    }
}
