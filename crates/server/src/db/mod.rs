//! Database boot: ensure the data directory, start hiqlite, migrate, wait.

pub mod cluster;
pub mod migrations;

use std::fs;
use std::time::{Duration, Instant};

use hiqlite::macros::CacheVariants;
use hiqlite::{Client, Node, NodeConfig, start_node_with_cache};
use tracing::{info, instrument};

pub use migrations::{MigrateError, Migrations};

use crate::config::{AppConfig, Mode};

/// Cache index for the hot paths. The session lookup on every authenticated
/// request is the one that earns a second raft group; nothing else is here yet.
#[derive(Debug, CacheVariants)]
pub enum AppCache {
    Sessions,
}

/// How long to wait for both raft groups to elect a leader before giving up.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);

/// Raft timings only a throwaway single-node instance should touch.
///
/// On a *fresh* data directory hiqlite waits `heartbeat_interval * 5` before
/// initialising each raft group, to be sure it is not rejoining a cluster after
/// losing its disk. Two groups at the 500 ms default is a flat 5 s on every
/// cold boot, which only ever-empty processes — tests — actually pay. Lowering
/// it is safe for one loopback node with no peer that could be mid-election,
/// and is exactly what must not be done in production.
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// `None` keeps hiqlite's 500 ms default.
    pub heartbeat_interval_ms: Option<u64>,
    /// `false` keeps cache WAL and snapshots in memory.
    pub cache_storage_disk: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: None,
            cache_storage_disk: true,
        }
    }
}

impl Tuning {
    /// For a single throwaway node that boots inside a test.
    #[must_use]
    pub fn fast_single_node() -> Self {
        Self {
            heartbeat_interval_ms: Some(50),
            cache_storage_disk: false,
        }
    }
}

/// Start the node described by `cfg` and the `RN_SITE_HQL_*` environment.
#[instrument(name = "db.open", skip(cfg, migrations), fields(mode = cfg.mode.as_str()))]
pub async fn open(cfg: &AppConfig, migrations: &Migrations) -> Result<Client, DbError> {
    let (raft, api) = cluster::listen_addrs();
    open_on(cfg, &raft, &api, Tuning::default(), migrations).await
}

/// [`open`] with the listen addresses and raft timings supplied explicitly, so
/// several nodes can run in one process without mutating the environment.
#[instrument(name = "db.open_on", skip(cfg, tuning, migrations), fields(mode = cfg.mode.as_str()))]
pub async fn open_on(
    cfg: &AppConfig,
    addr_raft: &str,
    addr_api: &str,
    tuning: Tuning,
    migrations: &Migrations,
) -> Result<Client, DbError> {
    let boot = Instant::now();
    ensure_data_dir(cfg)?;

    let client = start_node_with_cache::<AppCache>(node_config(cfg, addr_raft, addr_api, tuning)?)
        .await
        .map_err(DbError::Hiqlite)?;
    wait_until_healthy(&client, HEALTH_TIMEOUT).await?;
    migrations::apply(&client, migrations).await?;

    info!(
        latency_ms = boot.elapsed().as_secs_f64() * 1000.0,
        migrations = migrations.len(),
        "database ready"
    );
    Ok(client)
}

/// Block until both raft groups report a leader, or time out.
///
/// `start_node_with_cache` returning means the raft groups exist, not that they
/// can serve a statement. Without this wait the first query after boot races
/// the election and fails as `LeaderChange` — a confusing way to learn that the
/// node simply was not up yet.
pub async fn wait_until_healthy(client: &Client, timeout: Duration) -> Result<(), DbError> {
    let started = Instant::now();
    loop {
        let db = client.is_healthy_db().await;
        let cache = client.is_healthy_cache().await;
        if db.is_ok() && cache.is_ok() {
            info!(
                latency_ms = started.elapsed().as_secs_f64() * 1000.0,
                "raft groups healthy"
            );
            return Ok(());
        }
        if started.elapsed() >= timeout {
            let reason = match (db, cache) {
                (Err(err), _) => format!("db raft unhealthy: {err}"),
                (_, Err(err)) => format!("cache raft unhealthy: {err}"),
                _ => unreachable!("both arms are Ok only on the success path"),
            };
            return Err(DbError::Unhealthy(reason));
        }
        // One loopback node elects in milliseconds; upstream's 1 s floor would
        // dominate every test boot.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// hiqlite owns the state-machine file and creates it itself; the directory it
/// lives in is the deployment's responsibility. Dev creates it so `cargo run`
/// stays self-contained. Prod refuses, because an absent directory there means
/// the persistent volume did not mount and the alternative is a cluster that
/// silently forms on container-local storage.
pub fn ensure_data_dir(cfg: &AppConfig) -> Result<(), DbError> {
    let dir = cfg.db_dir();
    if dir.is_dir() {
        return Ok(());
    }
    if dir.exists() {
        return Err(DbError::config(format!(
            "the hiqlite data path is not a directory: {}",
            dir.display()
        )));
    }
    match cfg.mode {
        Mode::Dev => fs::create_dir_all(&dir).map_err(|source| DbError::Io {
            path: dir.display().to_string(),
            source,
        }),
        Mode::Prod => Err(DbError::config(format!(
            "mode=prod requires a pre-provisioned writable hiqlite data directory at {}",
            dir.display()
        ))),
    }
}

fn node_config(
    cfg: &AppConfig,
    addr_raft: &str,
    addr_api: &str,
    tuning: Tuning,
) -> Result<NodeConfig, DbError> {
    let topology = cluster::from_env(cfg, addr_raft, addr_api)?;
    let mut node_config = NodeConfig {
        node_id: topology.node_id,
        nodes: topology.nodes.iter().map(Node::from).collect(),
        data_dir: cfg.db_dir().to_string_lossy().into_owned().into(),
        filename_db: cfg.db_filename().into(),
        secret_raft: topology.secret_raft,
        secret_api: topology.secret_api,
        health_check_delay_secs: 0,
        cache_storage_disk: tuning.cache_storage_disk,
        learner_only: topology.learner_only,
        ..NodeConfig::default()
    };
    if let Some(heartbeat) = tuning.heartbeat_interval_ms {
        // Election timeouts must stay well above the heartbeat or a node calls
        // an election against itself between beats. hiqlite's own 3x/6x ratio.
        node_config.raft_config.heartbeat_interval = heartbeat;
        node_config.raft_config.election_timeout_min = heartbeat * 3;
        node_config.raft_config.election_timeout_max = heartbeat * 6;
    }
    Ok(node_config)
}

/// How many voters the configured peer map expects (one in local dev).
#[must_use]
pub fn expected_voters() -> usize {
    std::env::var("RN_SITE_HQL_NODES")
        .ok()
        .map(|nodes| {
            nodes
                .split(',')
                .filter(|node| !node.trim().is_empty())
                .count()
        })
        .filter(|count| *count > 0)
        .unwrap_or(1)
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("config error: {0}")]
    Config(String),
    #[error("the cluster did not become healthy: {0}")]
    Unhealthy(String),
    #[error(transparent)]
    Migrate(#[from] MigrateError),
    #[error(transparent)]
    Hiqlite(#[from] hiqlite::Error),
}

impl DbError {
    pub(crate) fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cfg(mode: Mode, db_path: PathBuf) -> AppConfig {
        AppConfig {
            mode,
            db_path,
            addr: "127.0.0.1:0".parse().unwrap(),
            static_dir: PathBuf::from("static"),
            id_key: Some("0".repeat(32)),
            bootstrap_operator_email: None,
        }
    }

    #[test]
    fn dev_creates_the_directory_and_leaves_the_database_file_to_hiqlite() {
        let root = tempfile::tempdir().unwrap();
        let db_path = root.path().join("nested/db.sqlite");
        ensure_data_dir(&cfg(Mode::Dev, db_path.clone())).unwrap();
        assert!(root.path().join("nested").is_dir());
        assert!(!db_path.exists());
    }

    #[test]
    fn prod_refuses_an_unprovisioned_data_directory() {
        let root = tempfile::tempdir().unwrap();
        let cfg = cfg(Mode::Prod, root.path().join("missing/db.sqlite"));
        assert!(matches!(ensure_data_dir(&cfg), Err(DbError::Config(_))));
    }

    #[test]
    fn a_data_path_whose_parent_is_a_file_is_refused_in_either_mode() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("occupied");
        std::fs::write(&file, b"not a directory").unwrap();
        for mode in [Mode::Dev, Mode::Prod] {
            let cfg = cfg(mode, file.join("db.sqlite"));
            assert!(matches!(ensure_data_dir(&cfg), Err(DbError::Config(_))));
        }
    }
}
