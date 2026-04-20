mod assets;
mod render;
mod routes;

use std::net::SocketAddr;

use anyhow::Context as _;
use axum::Router;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) manifest: assets::AssetManifest,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let dev_mode = std::env::var("RONITNATH_DEV").is_ok_and(|v| v == "1" || v == "true");
    let manifest = assets::AssetManifest::load("ui/dist/.vite/manifest.json", dev_mode);
    let state = AppState { manifest };

    let app = Router::new()
        .merge(routes::router())
        .nest_service("/ui/dist", ServeDir::new("ui/dist"))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_owned());
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .with_context(|| format!("invalid bind address {host}:{port}"))?;

    tracing::info!(%addr, "listening");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
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
