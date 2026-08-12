//! Graceful shutdown — axum signal pattern + hiqlite teardown.
//!
//! Mirrors `axum/examples/graceful-shutdown`: wait for Ctrl+C / SIGTERM, then
//! run process teardown. Hiqlite's docs say to leave the `shutdown-handle`
//! feature off when the app already owns signals, and call
//! [`Client::shutdown`](hiqlite::Client::shutdown) before exiting — that is
//! exactly what [`teardown`] does.

use std::time::Instant;

use hiqlite::Client;
use tokio::signal;
use tracing::{error, info, instrument};

/// Future suitable for [`axum::serve::Serve::with_graceful_shutdown`].
///
/// Waits for a shutdown signal, then runs [`teardown`] (hiqlite + future work).
pub async fn graceful_shutdown(db: Client, realtime: crate::realtime::RealtimeHub) {
    shutdown_signal().await;
    realtime.shutdown();
    teardown(db).await;
}

/// Ctrl+C and (on Unix) SIGTERM — same shape as the axum graceful-shutdown example.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            info!("shutdown signal: ctrl_c");
        }
        () = terminate => {
            info!("shutdown signal: terminate");
        }
    }
}

/// Process teardown. Hiqlite is always shut down here; add future arms to the
/// `select!` as other background work appears.
#[instrument(name = "process.teardown", skip(db))]
pub async fn teardown(db: Client) {
    info!("running process teardown");
    let started = Instant::now();

    tokio::select! {
        res = db.shutdown() => {
            match res {
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
        // Future teardown tasks (flushers, background workers, …) join here.
    }
}
