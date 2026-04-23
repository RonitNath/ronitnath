mod assets;
mod auth;
mod config;
mod db;
mod events;
mod render;
mod routes;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context as _;
use auth::{
    JwksClient, OidcConfig, SessionConfig as AuthSessionConfig, cookie_key_from_secret,
    initialize_session_store, oidc_router, session_layer,
};
use axum::http::{HeaderName, HeaderValue, header};
use axum::{Router, middleware};
use events::service::EventService;
use events::store::EventStore;
use events::tokens::TokenHasher;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) manifest: assets::AssetManifest,
    #[allow(dead_code)]
    pub(crate) domain: String,
    pub(crate) public_base_url: String,
    pub(crate) admins: config::AdminConfig,
    pub(crate) events: EventService,
    pub(crate) auth_ready: bool,
    pub(crate) dev_mode: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::load()?;
    let pool = db::open_pool(&cfg.database_url).await?;
    db::migrate(&pool).await?;

    let isoastra_configured = cfg.isoastra.is_configured();
    let explicit_dev_mode = std::env::var("RONITNATH_DEV").is_ok_and(|v| v == "1" || v == "true");
    let dev_mode = explicit_dev_mode || (cfg!(debug_assertions) && !isoastra_configured);
    if dev_mode && !explicit_dev_mode && !isoastra_configured {
        tracing::warn!("enabling dev auth bypass because Isoastra SSO is not configured");
    }
    let manifest = assets::AssetManifest::load("ui/dist/.vite/manifest.json", dev_mode);
    let event_store = EventStore::new(pool.clone());
    let event_service = EventService::new(
        event_store,
        TokenHasher::new(cfg.token_secret.as_bytes()),
        cfg.public_base_url.clone(),
    );

    let state = AppState {
        manifest,
        domain: cfg.domain.clone(),
        public_base_url: cfg.public_base_url.clone(),
        admins: cfg.admins.clone(),
        events: event_service,
        auth_ready: false,
        dev_mode,
    };

    let session_cfg = AuthSessionConfig {
        table_name: "http_sessions".to_owned(),
        cookie_name: cfg.session.cookie_name.clone(),
        cookie_secure: cfg.session.cookie_secure,
        expiry_hours: cfg.session.expiry_hours,
        cookie_domain: None,
    };
    let session_store = initialize_session_store(pool, &session_cfg).await?;
    let cookie_key = cookie_key_from_secret(cfg.token_secret.as_bytes());
    let session_mw = session_layer(session_store, cookie_key, &session_cfg);

    let oidc = OidcConfig {
        issuer: cfg.isoastra.issuer.clone(),
        client_id: cfg.isoastra.client_id.clone(),
        client_secret: cfg.isoastra.client_secret.clone(),
        redirect_uri: cfg.isoastra.redirect_uri.clone(),
        post_login_redirect: "/events".to_owned(),
        post_logout_redirect: "/events".to_owned(),
        expected_audience: cfg.isoastra.client_id.clone(),
    };
    let jwks = if isoastra_configured {
        match JwksClient::new(&cfg.isoastra.issuer).await {
            Ok(jwks) => Some(jwks),
            Err(err) => {
                tracing::warn!(error = %err, issuer = %cfg.isoastra.issuer, "isoastra jwks unavailable; auth routes disabled");
                None
            }
        }
    } else {
        tracing::warn!("isoastra sso is not configured; auth routes disabled");
        None
    };

    let mut app = Router::new()
        .merge(routes::router())
        .nest_service("/ui/dist", ServeDir::new("ui/dist"))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(AppState {
            auth_ready: jwks.is_some(),
            ..state
        });

    if let Some(jwks) = jwks {
        app = app.merge(oidc_router(oidc, Arc::clone(&jwks)));
    }

    let app = app
        .layer(middleware::from_fn(auth::optional_auth))
        .layer(session_mw)
        .layer(security_header(
            header::X_CONTENT_TYPE_OPTIONS,
            "nosniff",
        ))
        .layer(security_header(
            header::REFERRER_POLICY,
            "strict-origin-when-cross-origin",
        ))
        .layer(security_header(
            HeaderName::from_static("x-frame-options"),
            "DENY",
        ))
        .layer(security_header(
            HeaderName::from_static("content-security-policy"),
            "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
        ))
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(1)
                .burst_size(180)
                .key_extractor(SmartIpKeyExtractor)
                .finish()
                .context("build rate limiter")?,
        ))
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port)
        .parse()
        .with_context(|| format!("invalid bind address {}:{}", cfg.host, cfg.port))?;

    tracing::info!(%addr, domain = %cfg.domain, "listening");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

fn security_header(name: HeaderName, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(name, HeaderValue::from_static(value))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            let _ = sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown");
}
