//! Tracing setup: an env filter everywhere, JSON once the process is in prod.
//!
//! A local `cargo run` wants readable lines; a node behind the mesh feeds a log
//! collector that parses one object per line. The filter is the same in both:
//! `RUST_LOG` if set, otherwise `info` for us and `warn` for our dependencies.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::Mode;

/// Install the global subscriber. Calling this twice is a no-op by design so a
/// test that boots a second server does not panic on the second install.
pub fn init(mode: Mode) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("rn_site=info,tower_http=info,hiqlite=warn,warn"));
    let registry = tracing_subscriber::registry().with(filter);
    let installed = match mode {
        Mode::Prod => registry
            .with(tracing_subscriber::fmt::layer().json().flatten_event(true))
            .try_init(),
        Mode::Dev => registry
            .with(tracing_subscriber::fmt::layer().with_target(false))
            .try_init(),
    };
    if installed.is_err() {
        tracing::debug!("tracing subscriber was already installed");
    }
}
