//! Shared harness for the integration tests.
//!
//! Every test gets its own hiqlite node in its own temp directory on its own
//! pair of ports. That is heavier than sharing one node, and it is deliberate:
//! `#[tokio::test]` builds a fresh runtime per test, and a hiqlite client whose
//! background tasks were spawned on a runtime that has since been dropped is a
//! hang, not an error. Per-test isolation also means no test can see another's
//! rows, so registrations do not have to invent unique emails.
//!
//! What is *not* isolated is the timing registry in [`timing`] — it is global on
//! purpose, so the report at the end covers the whole run.

#![allow(dead_code)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::OnceLock;

use axum_test::TestServer;
use hiqlite::Client;
use rn_site::auth::{AuthState, CookieSecurity};
use rn_site::operations::{
    config::{AppConfig, Mode},
    db, telemetry,
};
use tempfile::TempDir;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub mod timing;

/// Password used by the flow tests. Long enough to clear the length gate.
pub const TEST_PASSWORD: &str = "correct horse battery staple";

/// Install the verbose-debug subscriber exactly once per test binary.
///
/// Defaults to `trace` for our own crate and `debug` for the HTTP layer, which
/// is what makes the guard's decision path — cookie present, session resolved,
/// capability held or not — readable in the captured output. `RUST_LOG` still
/// wins if it is set.
///
/// [`timing::TimingLayer`] rides along on the same subscriber, so installing
/// logging is what turns timing collection on.
pub fn init_tracing() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "rn_site=trace,tower_http=debug,axum::rejection=trace,info".into());

        let fmt = tracing_subscriber::fmt::layer()
            .with_test_writer()
            .with_target(true)
            .with_line_number(true)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE);

        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt)
            .with(timing::TimingLayer)
            .try_init();

        // One table for the whole binary, printed on the way out.
        timing::report_at_exit("rn-site auth integration run");
    });
}

/// A booted app: hiqlite node, auth router, and a cookie-keeping test client.
pub struct TestApp {
    pub server: TestServer,
    pub db: Client,
    pub state: AuthState,
    /// Held so the database directory outlives the node.
    _data_dir: TempDir,
}

impl TestApp {
    /// Boot a fresh node and build the real auth router in front of it.
    pub async fn boot() -> Self {
        init_tracing();

        let data_dir = tempfile::tempdir().expect("temp dir for hiqlite data");
        let cfg = AppConfig {
            mode: Mode::Dev,
            db_path: PathBuf::from(data_dir.path()).join("db.sqlite"),
        };

        let db = db::open_and_migrate_tuned(
            &cfg,
            free_addr(),
            free_addr(),
            db::NodeTuning::fast_single_node(),
        )
        .await
        .expect("boot hiqlite and migrate");

        // `Insecure` deliberately: the mock transport is not HTTPS, so a
        // `Secure` cookie would be set and then never sent back, and every
        // signed-in leg of the golden flow would read as a broken sign-in.
        let state = AuthState::new(db.clone(), CookieSecurity::Insecure);

        // The same `layer_http_trace` the binary applies, so the aggregate
        // report includes end-to-end request latency and not just the spans
        // inside the handlers.
        let app = telemetry::layer_http_trace(rn_site::auth::router(state.clone()));

        let server = TestServer::builder()
            // Without this the session cookie would not survive from the
            // sign-in response into the next request, and the whole flow would
            // read as "sign-in never works".
            .save_cookies()
            .build(app);

        Self {
            server,
            db,
            state,
            _data_dir: data_dir,
        }
    }

    /// Shut the node down cleanly so the temp dir can be removed without the
    /// raft log still being written into it.
    pub async fn shutdown(self) {
        let TestApp { db, _data_dir, .. } = self;
        if let Err(err) = db.shutdown().await {
            tracing::warn!(%err, "hiqlite shutdown failed in test teardown");
        }
        drop(_data_dir);
    }

    /// Stop and reopen the complete application against the same persistent
    /// Hiqlite directory. The HTTP client's cookie jar is intentionally new;
    /// restart-continuity tests replay the browser's saved cookie explicitly.
    pub async fn restart(self) -> Self {
        let TestApp {
            server,
            db,
            state,
            _data_dir,
        } = self;
        drop(server);
        drop(state);
        db.shutdown().await.expect("shutdown before restart");

        let cfg = AppConfig {
            mode: Mode::Dev,
            db_path: PathBuf::from(_data_dir.path()).join("db.sqlite"),
        };
        let db = db::open_and_migrate_tuned(
            &cfg,
            free_addr(),
            free_addr(),
            db::NodeTuning::fast_single_node(),
        )
        .await
        .expect("reopen hiqlite after restart");
        let state = AuthState::new(db.clone(), CookieSecurity::Insecure);
        let app = telemetry::layer_http_trace(rn_site::auth::router(state.clone()));
        let server = TestServer::builder().save_cookies().build(app);

        Self {
            server,
            db,
            state,
            _data_dir,
        }
    }
}

/// Ask the OS for an unused port and hand back `127.0.0.1:<port>`.
///
/// Racy in principle — the listener is closed before hiqlite binds — but the
/// window is microseconds and the alternative is hardcoded ports that collide
/// between concurrently running tests, which fails far more often.
fn free_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let port = listener.local_addr().expect("read bound address").port();
    drop(listener);
    format!("127.0.0.1:{port}")
}

/// A unique-ish address for a test that needs a second account.
pub fn unique_email(prefix: &str) -> String {
    format!("{prefix}-{}@example.test", uuid::Uuid::new_v4().simple())
}
