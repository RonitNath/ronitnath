//! Graceful shutdown: drain the listener on a signal, then close hiqlite.
//!
//! hiqlite's own docs say to leave the `shutdown-handle` feature off when the
//! application owns its signals and to call [`hiqlite::Client::shutdown`]
//! before exiting — a multi-node voter drains its raft membership there, and
//! skipping it turns an ordinary restart into an auto-heal boot.

use std::time::Instant;

use hiqlite::Client;
use tokio::signal;
use tracing::{error, info};

/// Resolves on Ctrl-C or SIGTERM. Hand this to
/// `axum::serve(..).with_graceful_shutdown(..)`: axum stops accepting and lets
/// in-flight requests finish, and only then does [`close`] run.
pub async fn signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("install the Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install the SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!(signal = "ctrl_c", "shutting down"),
        () = terminate => info!(signal = "terminate", "shutting down"),
    }
}

/// Close the database after the listener has drained.
pub async fn close(db: Client) {
    let started = Instant::now();
    match db.shutdown().await {
        Ok(()) => info!(
            latency_ms = started.elapsed().as_secs_f64() * 1000.0,
            "hiqlite shut down cleanly"
        ),
        Err(err) => error!(
            %err,
            latency_ms = started.elapsed().as_secs_f64() * 1000.0,
            "hiqlite shutdown failed"
        ),
    }
}
