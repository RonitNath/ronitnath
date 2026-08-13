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
fn cache_policy(path: &str) -> &'static str {
    if path.starts_with("/pkg/")
        || path.starts_with("/css/")
        || path.starts_with("/js/")
        || path.starts_with("/fonts/")
        || path.starts_with("/stars/")
        || path.starts_with("/sky/")
        || path.starts_with("/cities/")
        || path.starts_with("/images/")
    {
        "no-cache, must-revalidate"
    } else {
        "no-store"
    }
}

#[cfg(feature = "ssr")]
fn require_site_bundle(site_root: &std::path::Path) -> Result<(), String> {
    const REQUIRED: &[&str] = &[
        "pkg/rn-site.css",
        "pkg/rn-site.js",
        "pkg/rn-site.wasm",
        "css/site.css",
        "css/starscape.css",
        "js/starscape-ui.js",
        "js/starscape-explorer.js",
        "js/mini-globe.js",
        "stars/bright.bin",
        "stars/named.json",
    ];
    let missing = REQUIRED
        .iter()
        .filter(|relative| !site_root.join(relative).is_file())
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "site bundle is incomplete under {} (missing {}); run `cargo leptos build` or `cargo leptos watch`",
            site_root.display(),
            missing.join(", ")
        ))
    }
}

#[cfg(feature = "ssr")]
fn require_fresh_site_assets(
    public_root: &std::path::Path,
    site_root: &std::path::Path,
) -> Result<(), String> {
    const CRITICAL: &[&str] = &[
        "css/site.css",
        "css/starscape.css",
        "js/starscape-ui.js",
        "js/starscape-explorer.js",
        "js/mini-globe.js",
        "stars/bright.bin",
        "stars/named.json",
    ];
    let stale = CRITICAL
        .iter()
        .filter(|relative| {
            let source = public_root.join(relative);
            let generated = site_root.join(relative);
            std::fs::read(source).ok() != std::fs::read(generated).ok()
        })
        .copied()
        .collect::<Vec<_>>();
    if stale.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "generated site assets are stale ({}) under {}; run `cargo leptos build` or `cargo leptos watch`",
            stale.join(", "),
            site_root.display(),
        ))
    }
}

#[cfg(feature = "ssr")]
async fn cache_and_version_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{HeaderName, HeaderValue, header};

    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;
    let cache = cache_policy(&path);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response.headers_mut().insert(
        HeaderName::from_static("x-rn-app-version"),
        HeaderValue::from_str(&release_version()).expect("git revision is a valid header value"),
    );
    response
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

#[cfg(feature = "ssr")]
async fn validate_manage_route(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    if let Some(slug) = request.uri().path().strip_prefix("/manage/")
        && (slug.contains('/') || rn_site::manage::DataModel::parse(slug).is_none())
    {
        return (
            axum::http::StatusCode::NOT_FOUND,
            "The requested data model does not exist.",
        )
            .into_response();
    }
    next.run(request).await
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
    require_site_bundle(std::path::Path::new(leptos_options.site_root.as_ref()))
        .expect("complete Leptos site bundle");
    if app_config.mode == rn_site::operations::config::Mode::Dev
        && std::path::Path::new("public").is_dir()
    {
        require_fresh_site_assets(
            std::path::Path::new("public"),
            std::path::Path::new(leptos_options.site_root.as_ref()),
        )
        .expect("fresh generated site assets");
    }

    let db = db::open_and_migrate(&app_config)
        .await
        .expect("open database and migrate");

    // `Secure` on the session cookie follows the runtime mode: prod sits behind
    // the TLS-terminating edge, dev does not.
    let auth_state = AuthState::for_mode(db.clone(), app_config.mode);

    let star_lod_path =
        std::path::PathBuf::from(leptos_options.site_root.as_ref()).join("stars/lod/g12.bin");
    let routes = generate_route_list(App);
    let manage_routes = routes
        .iter()
        .filter(|route| route.path().starts_with("/manage"))
        .cloned()
        .collect::<Vec<_>>();
    let protected_routes = routes
        .iter()
        .filter(|route| route.path() == "/protected")
        .cloned()
        .collect::<Vec<_>>();
    let public_routes = routes
        .into_iter()
        .filter(|route| route.path() != "/protected" && !route.path().starts_with("/manage"))
        .collect::<Vec<_>>();

    let realtime_hub = rn_site::realtime::RealtimeHub::new(node_name());
    let _realtime_listener = realtime_hub.start(db.clone());
    let realtime_state = rn_site::realtime::RealtimeState::new(
        db.clone(),
        realtime_hub.clone(),
        release_version(),
        std::env::var("PUBLIC_URL").unwrap_or_else(|_| format!("http://{addr}")),
    );

    let render_context = {
        let auth_state = auth_state.clone();
        move || provide_context(auth_state.clone())
    };
    let render_shell = {
        let leptos_options = leptos_options.clone();
        move || shell(leptos_options.clone())
    };
    let protected_pages = Router::new()
        .leptos_routes_with_handler(
            protected_routes,
            leptos_axum::render_app_to_stream_with_context(
                render_context.clone(),
                render_shell.clone(),
            ),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            |st, req, next| {
                rn_site::auth::guard::require_capability(
                    rn_site::auth::Capability::TestAuth,
                    st,
                    req,
                    next,
                )
            },
        ))
        .with_state::<()>(leptos_options.clone());
    let manage_pages = Router::new()
        .leptos_routes_with_handler(
            manage_routes,
            leptos_axum::render_app_to_stream_with_context(
                render_context.clone(),
                render_shell.clone(),
            ),
        )
        .route_layer(axum::middleware::from_fn(validate_manage_route))
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            |st, req, next| {
                rn_site::auth::guard::require_capability(
                    rn_site::auth::Capability::Manage,
                    st,
                    req,
                    next,
                )
            },
        ))
        .with_state::<()>(leptos_options.clone());

    let readiness_db = db.clone();
    let readiness_realtime = realtime_hub.clone();
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
                let realtime = readiness_realtime.clone();
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
                    let realtime_listener = realtime.listener_healthy();
                    let healthy = db.is_healthy_db().await.is_ok()
                        && db.is_healthy_cache().await.is_ok()
                        && voters == expected
                        && cache_voters == expected
                        && realtime_listener;
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
                            "realtime_listener": realtime_listener,
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
        .leptos_routes_with_context(&leptos_options, public_routes, render_context, render_shell)
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state::<()>(leptos_options)
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
        .merge(protected_pages)
        .merge(manage_pages)
        .merge(rn_site::auth::api_router(auth_state))
        .merge(
            Router::new()
                .route("/api/realtime", get(rn_site::realtime::upgrade))
                .with_state(realtime_state),
        )
        .layer(axum::middleware::from_fn(cache_and_version_headers));
    let app = telemetry::layer_http_trace(app);

    info!(%addr, mode = app_config.mode.as_str(), "listening");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown::graceful_shutdown(db, realtime_hub))
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // Binary requires `--features ssr` (the package default). Hydrate builds
    // use the `hydrate` wasm entrypoint in lib.rs instead.
    eprintln!("rn-site binary requires the `ssr` feature (enabled by default)");
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::{cache_policy, require_fresh_site_assets, require_site_bundle};

    #[test]
    fn static_assets_revalidate_while_documents_and_apis_do_not_store() {
        assert_eq!(cache_policy("/css/site.css"), "no-cache, must-revalidate");
        assert_eq!(
            cache_policy("/pkg/rn-site.wasm"),
            "no-cache, must-revalidate"
        );
        assert_eq!(cache_policy("/manage"), "no-store");
        assert_eq!(cache_policy("/api/whoami"), "no-store");
        assert_eq!(cache_policy("/version"), "no-store");
    }

    #[test]
    fn incomplete_site_bundle_is_rejected_before_serving_broken_html() {
        let root = tempfile::tempdir().unwrap();
        let error = require_site_bundle(root.path()).unwrap_err();
        assert!(error.contains("pkg/rn-site.css"));

        for relative in [
            "pkg/rn-site.css",
            "pkg/rn-site.js",
            "pkg/rn-site.wasm",
            "css/site.css",
            "css/starscape.css",
            "js/starscape-ui.js",
            "js/starscape-explorer.js",
            "js/mini-globe.js",
            "stars/bright.bin",
            "stars/named.json",
        ] {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"fixture").unwrap();
        }
        require_site_bundle(root.path()).unwrap();
    }

    #[test]
    fn source_tree_dev_rejects_stale_generated_assets() {
        let public = tempfile::tempdir().unwrap();
        let site = tempfile::tempdir().unwrap();
        for relative in [
            "css/site.css",
            "css/starscape.css",
            "js/starscape-ui.js",
            "js/starscape-explorer.js",
            "js/mini-globe.js",
            "stars/bright.bin",
            "stars/named.json",
        ] {
            let source = public.path().join(relative);
            let generated = site.path().join(relative);
            std::fs::create_dir_all(source.parent().unwrap()).unwrap();
            std::fs::create_dir_all(generated.parent().unwrap()).unwrap();
            std::fs::write(source, b"current").unwrap();
            std::fs::write(generated, b"current").unwrap();
        }
        require_fresh_site_assets(public.path(), site.path()).unwrap();
        std::fs::write(site.path().join("js/starscape-ui.js"), b"old").unwrap();
        let error = require_fresh_site_assets(public.path(), site.path()).unwrap_err();
        assert!(error.contains("js/starscape-ui.js"));
        assert!(error.contains("cargo leptos build"));
    }
}
