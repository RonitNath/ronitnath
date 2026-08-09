//! Database boot: ensure paths, start hiqlite, migrate.
//!
//! Runtime uses hiqlite (on-disk SQLite state machine). Unit tests apply the
//! same migration SQL against an in-memory `rusqlite` connection.

use std::fs;
use std::time::{Duration, Instant};

use hiqlite::macros::CacheVariants;
use hiqlite::{
    Client, Error as HiqliteError, Node, NodeConfig, Row, params, start_node_with_cache,
};
use tracing::{info, info_span, instrument, warn};

use crate::auth::Migrations;
use crate::operations::config::{AppConfig, Mode};

/// Cache index for the session (and future) hot paths.
#[derive(Debug, CacheVariants)]
pub enum AppCache {
    Sessions,
}

/// Ensure the Hiqlite data directory exists, start Hiqlite, and run migrations.
///
/// Raft/API addresses come from `RN_SITE_HQL_ADDR_*`. Callers that need to pick
/// their own ports — integration tests, which run several nodes concurrently —
/// should use [`open_and_migrate_on`] instead of mutating process env.
#[instrument(name = "db.open_and_migrate", skip(cfg), fields(db_path = %cfg.db_path.display(), mode = cfg.mode.as_str()))]
pub async fn open_and_migrate(cfg: &AppConfig) -> Result<Client, DbError> {
    let raft_addr =
        std::env::var("RN_SITE_HQL_ADDR_RAFT").unwrap_or_else(|_| "127.0.0.1:8100".to_string());
    let api_addr =
        std::env::var("RN_SITE_HQL_ADDR_API").unwrap_or_else(|_| "127.0.0.1:8200".to_string());
    open_and_migrate_on(cfg, raft_addr, api_addr).await
}

/// [`open_and_migrate`] with the cluster addresses supplied explicitly.
#[instrument(name = "db.open_and_migrate_on", skip(cfg), fields(db_path = %cfg.db_path.display(), mode = cfg.mode.as_str()))]
pub async fn open_and_migrate_on(
    cfg: &AppConfig,
    raft_addr: String,
    api_addr: String,
) -> Result<Client, DbError> {
    open_and_migrate_tuned(cfg, raft_addr, api_addr, NodeTuning::default()).await
}

/// Knobs that only a single-node, loopback, throwaway deployment should touch.
///
/// The one that matters is [`Self::heartbeat_interval_ms`]. On a *fresh* data
/// directory hiqlite sleeps `heartbeat_interval * 5` before initialising each
/// raft group, to be sure it is not rejoining an existing cluster after data
/// loss (`init.rs::is_initialized_timeout`). We start two groups — db and cache
/// — so the default 500 ms heartbeat costs a flat 5 s on every boot. An
/// already-initialised node skips the wait entirely, which is why only tests,
/// which always start empty, pay it.
///
/// Lowering this is safe *only* because the test cluster is one node on
/// loopback with no peer that could be mid-election. Do not lower it in prod:
/// the wait is what stops a node that lost its disk from splitting the cluster.
#[derive(Debug, Clone, Copy)]
pub struct NodeTuning {
    /// `None` keeps hiqlite's 500 ms default.
    pub heartbeat_interval_ms: Option<u64>,
    /// `false` keeps cache WAL/snapshots in memory. Irrelevant for durability
    /// here — caches are always rebuilt — but it saves disk churn in tests.
    pub cache_storage_disk: bool,
}

impl Default for NodeTuning {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: None,
            cache_storage_disk: true,
        }
    }
}

impl NodeTuning {
    /// Tuning for a throwaway single-node instance that boots on every test.
    #[must_use]
    pub fn fast_single_node() -> Self {
        Self {
            heartbeat_interval_ms: Some(50),
            cache_storage_disk: false,
        }
    }
}

/// [`open_and_migrate_on`] with raft timings overridden. See [`NodeTuning`].
#[instrument(name = "db.open_and_migrate_tuned", skip(cfg), fields(db_path = %cfg.db_path.display(), mode = cfg.mode.as_str()))]
pub async fn open_and_migrate_tuned(
    cfg: &AppConfig,
    raft_addr: String,
    api_addr: String,
    tuning: NodeTuning,
) -> Result<Client, DbError> {
    let boot = Instant::now();

    ensure_db_paths(cfg)?;

    let node_config = node_config_for(cfg, raft_addr, api_addr, tuning)?;
    let client = {
        let span = info_span!("db.hiqlite_start");
        let _guard = span.enter();
        let started = Instant::now();
        let client = start_node_with_cache::<AppCache>(node_config)
            .await
            .map_err(DbError::Hiqlite)?;
        info!(
            latency_ms = started.elapsed().as_secs_f64() * 1000.0,
            "hiqlite node started"
        );
        client
    };

    wait_until_healthy(&client, HEALTH_TIMEOUT).await?;

    {
        let span = info_span!("db.migrate");
        let _guard = span.enter();
        let started = Instant::now();
        validate_runtime_migration_history(&client).await?;
        client
            .migrate::<Migrations>()
            .await
            .map_err(DbError::Hiqlite)?;
        info!(
            latency_ms = started.elapsed().as_secs_f64() * 1000.0,
            "database migrations applied"
        );
    }

    info!(
        latency_ms = boot.elapsed().as_secs_f64() * 1000.0,
        "database ready"
    );
    Ok(client)
}

/// How long to wait for the raft groups to elect a leader before giving up.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);

/// Block until both raft groups report a leader, or time out.
///
/// `start_node_with_cache` returning does not mean the node can serve a query —
/// it means the raft groups exist. Upstream's own test suite polls
/// `is_healthy_db`/`is_healthy_cache` in a loop for exactly this reason rather
/// than trusting startup to have finished. Without this, the first statement
/// after boot races the leader election and fails as `LeaderChange`, which is a
/// confusing way to learn that the node simply was not up yet.
#[instrument(name = "db.wait_until_healthy", skip(client))]
pub async fn wait_until_healthy(client: &Client, timeout: Duration) -> Result<(), DbError> {
    let started = Instant::now();
    let mut attempts = 0_u32;

    loop {
        attempts += 1;
        let db = client.is_healthy_db().await;
        let cache = client.is_healthy_cache().await;
        if db.is_ok() && cache.is_ok() {
            info!(
                attempts,
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
            warn!(attempts, %reason, "gave up waiting for a healthy cluster");
            return Err(DbError::Unhealthy(reason));
        }

        // Poll far faster than upstream's 1s: a single loopback node elects in
        // milliseconds, and a 1s floor would dominate every test boot.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Ensure Hiqlite's data directory exists.
///
/// Hiqlite owns the nested SQLite state-machine file and creates it itself.
/// Development keeps its self-contained `cargo run` behavior by creating the
/// directory. Production requires provisioning to create and mount it first,
/// preventing an accidental boot onto ephemeral container storage.
#[instrument(name = "db.ensure_paths", skip(cfg), fields(db_path = %cfg.db_path.display()))]
pub fn ensure_db_paths(cfg: &AppConfig) -> Result<(), DbError> {
    let started = Instant::now();
    let dir = cfg.db_dir();
    if !dir.exists() {
        match cfg.mode {
            Mode::Dev => {
                info!(path = %dir.display(), "creating database directory");
                fs::create_dir_all(&dir).map_err(|e| DbError::Io {
                    path: dir.display().to_string(),
                    source: e,
                })?;
            }
            Mode::Prod => {
                return Err(DbError::Config(format!(
                    "mode=prod requires a pre-provisioned writable Hiqlite data directory at {}",
                    dir.display()
                )));
            }
        }
    } else if !dir.is_dir() {
        return Err(DbError::Config(format!(
            "Hiqlite data path is not a directory: {}",
            dir.display()
        )));
    }

    info!(
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "database paths ensured"
    );
    Ok(())
}

fn node_config_for(
    cfg: &AppConfig,
    raft_addr: String,
    api_addr: String,
    tuning: NodeTuning,
) -> Result<NodeConfig, DbError> {
    let data_dir = cfg.db_dir();
    let filename_db = cfg.db_filename();

    let (node_id, nodes, secret_raft, secret_api, learner_only) = match cfg.mode {
        Mode::Dev => {
            warn!("hiqlite using built-in single-node dev configuration and secrets");
            (
                1,
                vec![Node {
                    id: 1,
                    addr_raft: raft_addr,
                    addr_api: api_addr,
                }],
                "rn-site-dev-raft-secret".to_string(),
                "rn-site-dev-api-secret".to_string(),
                false,
            )
        }
        Mode::Prod => {
            let node_id = required_env("RN_SITE_HQL_NODE_ID")?
                .parse()
                .map_err(|_| DbError::Config("RN_SITE_HQL_NODE_ID must be an integer".into()))?;
            let nodes = parse_cluster_nodes(&required_env("RN_SITE_HQL_NODES")?, node_id)?;
            let secret_raft =
                read_secret("RN_SITE_HQL_SECRET_RAFT", "RN_SITE_HQL_SECRET_RAFT_FILE")?;
            let secret_api = read_secret("RN_SITE_HQL_SECRET_API", "RN_SITE_HQL_SECRET_API_FILE")?;
            let learner_only = optional_bool_env("RN_SITE_HQL_LEARNER_ONLY")?;
            (node_id, nodes, secret_raft, secret_api, learner_only)
        }
    };

    let mut node_config = NodeConfig {
        node_id,
        nodes,
        data_dir: data_dir.to_string_lossy().into_owned().into(),
        filename_db: filename_db.into(),
        secret_raft,
        secret_api,
        health_check_delay_secs: 0,
        cache_storage_disk: tuning.cache_storage_disk,
        learner_only,
        ..NodeConfig::default()
    };

    if let Some(heartbeat) = tuning.heartbeat_interval_ms {
        // Election timeouts must stay well above the heartbeat or a node will
        // call an election against itself between beats. hiqlite's own ratio is
        // 3×/6×, kept here.
        node_config.raft_config.heartbeat_interval = heartbeat;
        node_config.raft_config.election_timeout_min = heartbeat * 3;
        node_config.raft_config.election_timeout_max = heartbeat * 6;
    }

    Ok(node_config)
}

fn required_env(name: &str) -> Result<String, DbError> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DbError::Config(format!("mode=prod requires {name}")))
}

fn optional_bool_env(name: &str) -> Result<bool, DbError> {
    match std::env::var(name).ok().as_deref().map(str::trim) {
        None | Some("") | Some("false" | "0") => Ok(false),
        Some("true" | "1") => Ok(true),
        Some(_) => Err(DbError::Config(format!("{name} must be true/false or 1/0"))),
    }
}

/// Load a cluster secret from a mounted file (preferred) or a process variable.
fn read_secret(value_name: &str, file_name: &str) -> Result<String, DbError> {
    let secret = if let Ok(path) = std::env::var(file_name) {
        fs::read_to_string(&path)
            .map_err(|source| DbError::Io { path, source })?
            .trim()
            .to_string()
    } else {
        required_env(value_name)?
    };

    if secret.len() < 16 {
        return Err(DbError::Config(format!(
            "{value_name} must contain at least 16 characters"
        )));
    }
    Ok(secret)
}

/// Parse `id@raft-address@api-address` entries separated by commas.
///
/// Requiring an odd production topology avoids accidentally operating a
/// two/four-voter cluster with a needlessly larger quorum. Expansion from
/// three to five is represented by adding both new nodes before they join.
fn parse_cluster_nodes(value: &str, local_id: u64) -> Result<Vec<Node>, DbError> {
    let mut nodes = Vec::new();
    for entry in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let mut fields = entry.split('@');
        let id = fields
            .next()
            .and_then(|id| id.parse::<u64>().ok())
            .ok_or_else(|| {
                DbError::Config(format!(
                    "invalid RN_SITE_HQL_NODES entry `{entry}`; expected id@raft@api"
                ))
            })?;
        let addr_raft = fields.next().filter(|s| !s.is_empty()).ok_or_else(|| {
            DbError::Config(format!(
                "invalid RN_SITE_HQL_NODES entry `{entry}`; missing raft address"
            ))
        })?;
        let addr_api = fields.next().filter(|s| !s.is_empty()).ok_or_else(|| {
            DbError::Config(format!(
                "invalid RN_SITE_HQL_NODES entry `{entry}`; missing API address"
            ))
        })?;
        if fields.next().is_some() {
            return Err(DbError::Config(format!(
                "invalid RN_SITE_HQL_NODES entry `{entry}`; expected id@raft@api"
            )));
        }
        if nodes.iter().any(|node: &Node| node.id == id) {
            return Err(DbError::Config(format!(
                "RN_SITE_HQL_NODES contains duplicate node id {id}"
            )));
        }
        for address in [addr_raft, addr_api] {
            if address.starts_with("127.")
                || address.starts_with("0.0.0.0")
                || address.starts_with("localhost")
            {
                return Err(DbError::Config(format!(
                    "production peer address must be mesh-reachable, not `{address}`"
                )));
            }
        }
        nodes.push(Node {
            id,
            addr_raft: addr_raft.to_string(),
            addr_api: addr_api.to_string(),
        });
    }

    if nodes.len() < 3 || nodes.len() % 2 == 0 {
        return Err(DbError::Config(format!(
            "RN_SITE_HQL_NODES must describe an odd cluster of at least 3 nodes; found {}",
            nodes.len()
        )));
    }
    if !nodes.iter().any(|node| node.id == local_id) {
        return Err(DbError::Config(format!(
            "RN_SITE_HQL_NODE_ID {local_id} is absent from RN_SITE_HQL_NODES"
        )));
    }
    Ok(nodes)
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
    #[error("cluster did not become healthy: {0}")]
    Unhealthy(String),
    #[error(transparent)]
    MigrationIntegrity(#[from] MigrationIntegrityError),
    #[error(transparent)]
    Hiqlite(#[from] HiqliteError),
    #[error(transparent)]
    Rusqlite(#[from] rusqlite::Error),
}

/// A previously-applied migration no longer matches the embedded history.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "migration {migration_id} integrity check failed for {field}: expected {expected}, found {actual}"
)]
pub struct MigrationIntegrityError {
    pub migration_id: u32,
    pub field: &'static str,
    pub expected: String,
    pub actual: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MigrationRecord {
    id: u32,
    name: String,
    hash: String,
}

#[derive(Clone, Debug)]
struct EmbeddedMigration {
    record: MigrationRecord,
    filename: String,
    content: Vec<u8>,
}

fn integrity_error(
    migration_id: u32,
    field: &'static str,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> MigrationIntegrityError {
    MigrationIntegrityError {
        migration_id,
        field,
        expected: expected.into(),
        actual: actual.into(),
    }
}

fn embedded_migrations() -> Result<Vec<EmbeddedMigration>, MigrationIntegrityError> {
    let mut migrations = Vec::new();

    for filename in Migrations::iter() {
        let filename = filename.into_owned();
        let (id, rest) = filename
            .split_once('_')
            .ok_or_else(|| integrity_error(0, "filename", "<integer>_<name>.sql", &filename))?;
        let id = id
            .parse::<u32>()
            .map_err(|_| integrity_error(0, "id", "a positive integer", id))?;
        let name = rest
            .strip_suffix(".sql")
            .ok_or_else(|| integrity_error(id, "filename", "a .sql suffix", &filename))?;
        let file = Migrations::get(&filename)
            .ok_or_else(|| integrity_error(id, "embedded file", &filename, "missing"))?;

        migrations.push(EmbeddedMigration {
            record: MigrationRecord {
                id,
                name: name.to_string(),
                hash: hex::encode(file.metadata.sha256_hash()),
            },
            filename,
            content: file.data.into_owned(),
        });
    }

    migrations.sort_by_key(|migration| migration.record.id);
    validate_sequence(
        migrations.iter().map(|migration| &migration.record),
        "embedded sequence",
    )?;
    Ok(migrations)
}

fn validate_sequence<'a>(
    records: impl IntoIterator<Item = &'a MigrationRecord>,
    field: &'static str,
) -> Result<(), MigrationIntegrityError> {
    for (index, record) in records.into_iter().enumerate() {
        let expected = u32::try_from(index + 1).expect("migration count fits u32");
        if record.id != expected {
            return Err(integrity_error(
                expected,
                field,
                expected.to_string(),
                record.id.to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_migration_history(
    embedded: &[MigrationRecord],
    applied: &[MigrationRecord],
) -> Result<(), MigrationIntegrityError> {
    validate_sequence(embedded, "embedded sequence")?;
    validate_sequence(applied, "applied sequence")?;

    for applied in applied {
        let Some(expected) = embedded.get(applied.id as usize - 1) else {
            return Err(integrity_error(
                applied.id,
                "embedded migration",
                format!("{}_{}", applied.id, applied.name),
                "missing",
            ));
        };
        if applied.name != expected.name {
            return Err(integrity_error(
                applied.id,
                "name",
                &expected.name,
                &applied.name,
            ));
        }
        if applied.hash != expected.hash {
            return Err(integrity_error(
                applied.id,
                "hash",
                &expected.hash,
                &applied.hash,
            ));
        }
    }
    Ok(())
}

struct MigrationTableExists(bool);

impl From<&mut Row<'_>> for MigrationTableExists {
    fn from(row: &mut Row<'_>) -> Self {
        Self(row.get::<i64>("present") != 0)
    }
}

impl From<&mut Row<'_>> for MigrationRecord {
    fn from(row: &mut Row<'_>) -> Self {
        Self {
            id: row.get::<i64>("id") as u32,
            name: row.get("name"),
            hash: row.get("hash"),
        }
    }
}

async fn validate_runtime_migration_history(client: &Client) -> Result<(), DbError> {
    let exists: MigrationTableExists = client
        .query_map_one(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_migrations'
             ) AS present",
            params!(),
        )
        .await?;
    if !exists.0 {
        return Ok(());
    }

    let applied: Vec<MigrationRecord> = client
        .query_map(
            "SELECT id, name, hash FROM _migrations ORDER BY id ASC",
            params!(),
        )
        .await?;
    let embedded = embedded_migrations()?;
    let records: Vec<_> = embedded
        .iter()
        .map(|migration| migration.record.clone())
        .collect();
    validate_migration_history(&records, &applied)?;
    Ok(())
}

/// Apply embedded migrations to an in-memory SQLite connection (tests).
#[instrument(name = "db.open_memory_migrated")]
pub fn open_memory_migrated() -> Result<rusqlite::Connection, DbError> {
    let started = Instant::now();
    let conn = rusqlite::Connection::open_in_memory()?;
    apply_migrations_rusqlite(&conn)?;
    info!(
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "in-memory migrations applied"
    );
    Ok(conn)
}

#[instrument(name = "db.apply_migrations_rusqlite", skip(conn))]
fn apply_migrations_rusqlite(conn: &rusqlite::Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            ts INTEGER NOT NULL,
            hash TEXT NOT NULL
        );",
    )?;

    let embedded = embedded_migrations()?;
    let mut statement = conn.prepare("SELECT id, name, hash FROM _migrations ORDER BY id ASC")?;
    let applied: Vec<MigrationRecord> = statement
        .query_map([], |row| {
            Ok(MigrationRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                hash: row.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    drop(statement);

    let records: Vec<_> = embedded
        .iter()
        .map(|migration| migration.record.clone())
        .collect();
    validate_migration_history(&records, &applied)?;

    for migration in embedded.into_iter().skip(applied.len()) {
        let sql = std::str::from_utf8(&migration.content).expect("migration is utf-8");
        let started = Instant::now();
        conn.execute_batch(sql)?;
        info!(
            migration = %migration.filename,
            latency_ms = started.elapsed().as_secs_f64() * 1000.0,
            "applied migration"
        );

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after 1970")
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO _migrations (id, name, ts, hash) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                migration.record.id,
                migration.record.name,
                ts,
                migration.record.hash
            ],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u32, name: &str, hash: &str) -> MigrationRecord {
        MigrationRecord {
            id,
            name: name.to_string(),
            hash: hash.to_string(),
        }
    }

    #[test]
    fn memory_migrations_create_auth_tables() {
        let conn = open_memory_migrated().expect("in-mem migrate");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='identities'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let kinds_ok: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('accounts') WHERE name='primary_for_identity_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kinds_ok, 1);

        let password_hash: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('identity_passwords')
                 WHERE name='password_hash'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(password_hash, 1);
    }

    #[test]
    fn dev_creates_only_the_data_directory_not_a_placeholder_database() {
        let root = tempfile::tempdir().unwrap();
        let db_path = root.path().join("nested/db.sqlite");
        let cfg = AppConfig {
            mode: Mode::Dev,
            db_path: db_path.clone(),
        };

        ensure_db_paths(&cfg).unwrap();
        assert!(root.path().join("nested").is_dir());
        assert!(!db_path.exists());
    }

    #[test]
    fn prod_refuses_an_unprovisioned_data_directory() {
        let root = tempfile::tempdir().unwrap();
        let cfg = AppConfig {
            mode: Mode::Prod,
            db_path: root.path().join("missing/db.sqlite"),
        };

        assert!(matches!(ensure_db_paths(&cfg), Err(DbError::Config(_))));
    }

    #[test]
    fn production_peer_map_requires_a_present_local_node_and_odd_quorum() {
        let three = "1@10.0.0.1:8100@10.0.0.1:8200,\
                     2@10.0.0.2:8100@10.0.0.2:8200,\
                     3@10.0.0.3:8100@10.0.0.3:8200";
        let parsed = parse_cluster_nodes(three, 2).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[1].id, 2);

        assert!(parse_cluster_nodes(three, 4).is_err());
        assert!(
            parse_cluster_nodes(
                "1@10.0.0.1:8100@10.0.0.1:8200,2@10.0.0.2:8100@10.0.0.2:8200",
                1
            )
            .is_err()
        );
        assert!(parse_cluster_nodes(
            "1@127.0.0.1:8100@10.0.0.1:8200,2@10.0.0.2:8100@10.0.0.2:8200,3@10.0.0.3:8100@10.0.0.3:8200",
            1
        )
        .is_err());
    }

    #[test]
    fn memory_migrations_are_idempotent() {
        let conn = open_memory_migrated().unwrap();
        apply_migrations_rusqlite(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn migration_history_accepts_an_embedded_append() {
        let embedded = vec![record(1, "auth", "aaa"), record(2, "sessions", "bbb")];
        let applied = vec![record(1, "auth", "aaa")];

        assert_eq!(validate_migration_history(&embedded, &applied), Ok(()));
    }

    #[test]
    fn migration_history_rejects_changed_content() {
        let embedded = vec![record(1, "auth", "new-hash")];
        let applied = vec![record(1, "auth", "old-hash")];

        let error = validate_migration_history(&embedded, &applied).unwrap_err();
        assert_eq!(error.migration_id, 1);
        assert_eq!(error.field, "hash");
        assert_eq!(error.expected, "new-hash");
        assert_eq!(error.actual, "old-hash");
    }

    #[test]
    fn migration_history_rejects_changed_name() {
        let embedded = vec![record(1, "accounts", "aaa")];
        let applied = vec![record(1, "auth", "aaa")];

        let error = validate_migration_history(&embedded, &applied).unwrap_err();
        assert_eq!(error.migration_id, 1);
        assert_eq!(error.field, "name");
    }

    #[test]
    fn migration_history_rejects_missing_embedded_migration() {
        let embedded = vec![record(1, "auth", "aaa")];
        let applied = vec![record(1, "auth", "aaa"), record(2, "sessions", "bbb")];

        let error = validate_migration_history(&embedded, &applied).unwrap_err();
        assert_eq!(error.migration_id, 2);
        assert_eq!(error.field, "embedded migration");
        assert_eq!(error.actual, "missing");
    }

    #[test]
    fn migration_history_rejects_embedded_and_applied_gaps() {
        let embedded_gap = vec![record(1, "auth", "aaa"), record(3, "later", "ccc")];
        let error = validate_migration_history(&embedded_gap, &[]).unwrap_err();
        assert_eq!(error.migration_id, 2);
        assert_eq!(error.field, "embedded sequence");

        let embedded = vec![
            record(1, "auth", "aaa"),
            record(2, "sessions", "bbb"),
            record(3, "later", "ccc"),
        ];
        let applied_gap = vec![record(1, "auth", "aaa"), record(3, "later", "ccc")];
        let error = validate_migration_history(&embedded, &applied_gap).unwrap_err();
        assert_eq!(error.migration_id, 2);
        assert_eq!(error.field, "applied sequence");
    }

    #[test]
    fn rusqlite_migrator_rejects_a_changed_applied_hash() {
        let conn = open_memory_migrated().unwrap();
        conn.execute("UPDATE _migrations SET hash = 'tampered' WHERE id = 1", [])
            .unwrap();

        let error = apply_migrations_rusqlite(&conn).unwrap_err();
        let DbError::MigrationIntegrity(error) = error else {
            panic!("expected migration integrity error");
        };
        assert_eq!(error.migration_id, 1);
        assert_eq!(error.field, "hash");
        assert_eq!(error.actual, "tampered");
    }
}
