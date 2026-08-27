//! Boot: read the configuration, start the database, serve, drain, close.

use std::time::Duration;

use rn_site::config::AppConfig;
use rn_site::db::Migrations;
use rn_site::{AppState, db, shutdown, telemetry};
use tracing::info;

#[tokio::main]
async fn main() {
    let config = match AppConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("rn-site: {error}");
            std::process::exit(1);
        }
    };
    telemetry::init(config.mode);

    // Derived, never stored: reading it at boot is what makes a missing key in
    // production a refusal to start rather than a page of unresolvable ids.
    let _id_key = config.require_id_key();

    // The schema is the kernel's. K1 hands over `rn_kernel::migrations()`; the
    // seam is this value, so adopting it is this line and nothing else.
    let migrations = Migrations::default();

    let db = match db::open(&config, &migrations).await {
        Ok(db) => db,
        Err(error) => {
            eprintln!("rn-site: database unavailable: {error}");
            std::process::exit(1);
        }
    };

    let addr = config.addr;
    let state = AppState::new(db.clone(), config);
    let app = rn_site::router(state.clone());

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("rn-site: cannot bind {addr}: {error}");
            shutdown::close(db).await;
            std::process::exit(1);
        }
    };
    info!(
        %addr,
        mode = state.config.mode.as_str(),
        node = %state.node,
        version = %state.version,
        "listening"
    );

    let served = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown::signal())
        .await;
    if let Err(error) = served {
        tracing::error!(%error, "the listener stopped with an error");
    }

    // The listener has drained by the time `serve` returns; only now is it safe
    // to hand the raft group back, which is what lets a voter leave cleanly.
    tokio::time::timeout(Duration::from_secs(20), shutdown::close(db))
        .await
        .unwrap_or_else(|_| tracing::error!("hiqlite did not shut down within 20s"));
}
