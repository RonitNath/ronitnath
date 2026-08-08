/// Which revision this process is, stamped into the image at build time.
///
/// `ARG` is scoped to the stage that declares it, so the Containerfile
/// re-declares `SOURCE_GIT_HASH` in the runtime stage; without that every node
/// reports `dev`, which is the one question these endpoints exist to answer.
#[cfg(feature = "ssr")]
fn release_version() -> String {
    std::env::var("SOURCE_GIT_HASH").unwrap_or_else(|_| "dev".to_string())
}

/// Which replica this process is, from the node's `node.env`.
#[cfg(feature = "ssr")]
fn node_name() -> String {
    std::env::var("RN_SITE_NODE").unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::response::IntoResponse;
    use axum::{Json, Router, routing::get};
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use rn_site::app::*;
    use rn_site::operations::{config::AppConfig, db, secrets, shutdown, telemetry};
    use tracing::info;

    telemetry::init();

    // Durable session signing key: load from `.env`, or generate and write it.
    let cookie_secret = secrets::ensure_cookie_secret_default()
        .expect("ensure COOKIE_SECRET in .env");
    // Held for session middleware once that lands; do not log the value.
    let _cookie_secret = cookie_secret;

    let app_config = AppConfig::load().expect("load config.toml / RN_SITE__*");
    let db = db::open_and_migrate(&app_config)
        .await
        .expect("open database and migrate");

    // `Some("Cargo.toml")` so plain `cargo run` works without cargo-leptos
    // injecting LEPTOS_OUTPUT_NAME.
    let conf = get_configuration(Some("Cargo.toml")).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let app = Router::new()
        // Liveness: no dependencies, no allocation of consequence, no reason to
        // ever fail while the process is up. This is what the edge's active
        // health check polls twice a second, so it must stay this cheap — and
        // what Compose's healthcheck uses, which must not restart a container
        // for a reason that readiness would report instead.
        .route("/healthz", get(|| async { "ok" }))
        // Readiness, and the answer to "which revision is nyc serving" without
        // an ssh. The rollout asserts every replica reports the same version.
        .route(
            "/readyz",
            get(|| async {
                Json(serde_json::json!({
                    "status": "ok",
                    "node": node_name(),
                    "version": release_version(),
                }))
                .into_response()
            }),
        )
        .route("/version", get(|| async { release_version() }))
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);
    let app = telemetry::layer_http_trace(app);

    info!(%addr, mode = app_config.mode.as_str(), "listening");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown::graceful_shutdown(db))
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // Binary requires `--features ssr` (the package default). Hydrate builds
    // use the `hydrate` wasm entrypoint in lib.rs instead.
    eprintln!("rn-site binary requires the `ssr` feature (enabled by default)");
}
