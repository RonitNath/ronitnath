//! Boot: read the configuration, start the database, serve, drain, close.

use std::time::Duration;

use rn_site::config::AppConfig;
use rn_site::{AppState, db, shutdown, telemetry};
use tracing::info;

/// The argument after `name` on the command line, if `name` is the subcommand.
fn subcommand(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    (args.next().as_deref() == Some(name)).then(|| args.next().unwrap_or_default())
}

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

    // The schema is the kernel's, and the seam is this value: the drift check
    // that runs before it (`db::migrations`) is the server's, the history it
    // checks is `rn-kernel`'s, and neither knows about the other.
    let migrations = match db::Migrations::embedded::<rn_kernel::Migrations>() {
        Ok(migrations) => migrations,
        Err(error) => {
            eprintln!("rn-site: the embedded migration history is malformed: {error}");
            std::process::exit(1);
        }
    };

    let db = match db::open(&config, &migrations).await {
        Ok(db) => db,
        Err(error) => {
            eprintln!("rn-site: database unavailable: {error}");
            std::process::exit(1);
        }
    };

    let addr = config.addr;
    let state = AppState::new(db.clone(), config);

    // `rn-site bootstrap-operator <email>` — the first operator, and nothing
    // else: the kernel refuses once one exists (`rn_site::bootstrap`).
    if let Some(email) = subcommand("bootstrap-operator") {
        let outcome = rn_site::bootstrap::operator(&state, &email).await;
        shutdown::close(db).await;
        match outcome {
            Ok(person) => println!("rn-site: {email} is now a platform operator ({person:?})"),
            Err(error) => {
                eprintln!("rn-site: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    // The observation lane: `last_seen`, match scanning and the two sweeps.
    // Held rather than detached, so it stops when this scope does — a task
    // still writing while hiqlite is being handed back is an auto-heal boot.
    let lane = rn_site::observe::spawn(state.clone());

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

    lane.stop().await;

    // The listener has drained by the time `serve` returns; only now is it safe
    // to hand the raft group back, which is what lets a voter leave cleanly.
    tokio::time::timeout(Duration::from_secs(20), shutdown::close(db))
        .await
        .unwrap_or_else(|_| tracing::error!("hiqlite did not shut down within 20s"));
}
