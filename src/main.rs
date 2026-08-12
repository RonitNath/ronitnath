/// Which revision this process is, stamped into the image at build time.
///
/// `ARG` is scoped to the stage that declares it, so the Containerfile
/// re-declares `SOURCE_GIT_HASH` in the runtime stage; without that every node
/// reports `dev`, which is the one question these endpoints exist to answer.
#[cfg(feature = "ssr")]
fn release_version() -> String {
    std::env::var("SOURCE_GIT_HASH").unwrap_or_else(|_| "dev".to_string())
}

#[cfg(feature = "ssr")]
async fn serve_star_lod_range(
    path: std::path::PathBuf,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{StatusCode, header};
    use axum::response::IntoResponse;
    use std::io::{Read, Seek};

    let Some(range) = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("bytes="))
        .filter(|value| !value.contains(','))
        .and_then(|value| value.split_once('-'))
        .and_then(|(start, end)| Some((start.parse::<u64>().ok()?, end.parse::<u64>().ok()?)))
    else {
        return (StatusCode::BAD_REQUEST, "a single byte range is required").into_response();
    };

    let result = tokio::task::spawn_blocking(move || -> std::io::Result<(u64, Vec<u8>)> {
        let mut file = std::fs::File::open(path)?;
        let file_len = file.metadata()?.len();
        let (start, end) = range;
        let requested = end.checked_sub(start).and_then(|size| size.checked_add(1));
        let Some(length) = requested.filter(|length| *length <= 2 * 1024 * 1024) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid or excessive range",
            ));
        };
        if end >= file_len {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "range outside file",
            ));
        }
        file.seek(std::io::SeekFrom::Start(start))?;
        let mut bytes = vec![0; length as usize];
        file.read_exact(&mut bytes)?;
        Ok((file_len, bytes))
    })
    .await;

    let Ok(Ok((file_len, bytes))) = result else {
        return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    };
    let end = range.1;
    axum::response::Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(
            header::CONTENT_RANGE,
            format!("bytes {}-{end}/{file_len}", range.0),
        )
        .header(header::CONTENT_LENGTH, bytes.len())
        .body(Body::from(bytes))
        .expect("valid star LOD range response")
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
    // injecting LEPTOS_OUTPUT_NAME. The runtime image ships no Cargo.toml —
    // there, configuration is environment-only, exactly like the container's
    // LEPTOS_* variables define it.
    let conf = if std::path::Path::new("Cargo.toml").exists() {
        get_configuration(Some("Cargo.toml"))
    } else {
        get_configuration(None)
    }
    .expect("leptos configuration");
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let star_lod_path =
        std::path::PathBuf::from(leptos_options.site_root.as_ref()).join("stars/lod/g12.bin");
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
                            // The cache raft is a separate group with its own
                            // membership; a readiness 503 with voters=3 and no
                            // cache count was exactly the blind spot that made
                            // the first formation failure unreadable.
                            "cache_voters": cache_voters,
                            "expected_voters": expected,
                        })),
                    )
                        .into_response()
                }
            }),
        )
        .route("/version", get(|| async { release_version() }))
        .route(
            "/stars/lod/g12.bin",
            get({
                let path = star_lod_path.clone();
                move |headers: axum::http::HeaderMap| serve_star_lod_range(path.clone(), headers)
            }),
        )
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
        // Resolve the session for everything above — the SSR pages — so the
        // top-bar nav can offer only what this caller may open. It attaches a
        // session and never refuses, so `/healthz`, `/readyz` and an anonymous
        // home page are unaffected. Applied here rather than to the whole app
        // because `auth::router`'s own guarded routes resolve the session
        // themselves; merging after this layer is what keeps them from paying
        // for it twice.
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            rn_site::auth::guard::attach_session,
        ))
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
