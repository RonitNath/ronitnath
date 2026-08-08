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
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use rn_site::app::*;

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // Generate the list of routes in your Leptos App
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

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}
