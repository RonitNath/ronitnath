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

/// Expected voter count from the configured peer map (one in local dev).
#[cfg(feature = "ssr")]
fn expected_voters() -> usize {
    std::env::var("RN_SITE_HQL_NODES")
        .ok()
        .map(|nodes| {
            nodes
                .split(',')
                .filter(|node| !node.trim().is_empty())
                .count()
        })
        .unwrap_or(1)
}

/// `rn-site` — serve the site, or run an operator command and exit.
#[cfg(feature = "ssr")]
#[derive(clap::Parser)]
#[command(name = "rn-site")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[cfg(feature = "ssr")]
#[derive(clap::Subcommand)]
enum Command {
    /// Operator commands against the local database. These open the same
    /// hiqlite directory the server uses — run them while the server is
    /// stopped.
    Admin {
        #[command(subcommand)]
        command: rn_site::operations::admin::AdminCommand,
    },
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::{Json, Router, routing::get, routing::post};
    use clap::Parser;
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use rn_site::app::*;
    use rn_site::auth::AuthState;
    use rn_site::operations::{admin, config::AppConfig, db, shutdown, telemetry};
    use tracing::info;

    let cli = Cli::parse();
    telemetry::init();

    let app_config = AppConfig::load().expect("load config.toml / RN_SITE__*");

    if let Some(Command::Admin { command }) = cli.command {
        if let Err(err) = admin::run(&app_config, command).await {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
        return;
    }

    let db = db::open_and_migrate(&app_config)
        .await
        .expect("open database and migrate");

    // `Secure` on the session cookie follows the runtime mode: prod sits behind
    // the TLS-terminating edge, dev does not.
    let auth_state = AuthState::for_mode(db.clone(), app_config.mode);

    // `Some("Cargo.toml")` so plain `cargo run` works without cargo-leptos
    // injecting LEPTOS_OUTPUT_NAME.
    let conf = get_configuration(Some("Cargo.toml")).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let readiness_db = db.clone();
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
            get(move || {
                let db = readiness_db.clone();
                async move {
                    let expected = expected_voters();
                    let db_metrics = db.metrics_db().await.ok();
                    let cache_metrics = db.metrics_cache().await.ok();
                    let voters = db_metrics
                        .as_ref()
                        .map(|metrics| metrics.membership_config.voter_ids().count())
                        .unwrap_or(0);
                    let cache_voters = cache_metrics
                        .as_ref()
                        .map(|metrics| metrics.membership_config.voter_ids().count())
                        .unwrap_or(0);
                    let leader = db_metrics
                        .as_ref()
                        .and_then(|metrics| metrics.current_leader);
                    let healthy = db.is_healthy_db().await.is_ok()
                        && db.is_healthy_cache().await.is_ok()
                        && voters == expected
                        && cache_voters == expected;
                    let status = if healthy {
                        StatusCode::OK
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    };
                    (
                        status,
                        Json(serde_json::json!({
                            "status": if healthy { "ok" } else { "unavailable" },
                            "node": node_name(),
                            "version": release_version(),
                            "leader": leader,
                            "voters": voters,
                            "expected_voters": expected,
                        })),
                    )
                        .into_response()
                }
            }),
        )
        .route("/version", get(|| async { release_version() }))
        // Server functions (the `/auth` sign-in) need `AuthState` in context.
        // The same closure is provided here and to `leptos_routes_with_context`
        // below: SSR calls server fns through the renderer, the browser calls
        // them through this route, and both must see the same context. The
        // wildcard coexists with the explicit `/api/auth/*` routes merged
        // later — static routes win over the wildcard.
        .route("/api/{*fn_name}", {
            let additional_context = {
                let auth_state = auth_state.clone();
                move || provide_context(auth_state.clone())
            };
            post(move |request| {
                leptos_axum::handle_server_fns_with_context(additional_context.clone(), request)
            })
        })
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let auth_state = auth_state.clone();
                move || provide_context(auth_state.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options)
        // Merged after `with_state` because the auth routes carry their own
        // `AuthState`; both sides are `Router<()>` by this point. Only this
        // side has a fallback, which is what keeps `merge` from panicking.
        .merge(rn_site::auth::router(auth_state));
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
