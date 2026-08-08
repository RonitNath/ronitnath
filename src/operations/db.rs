//! Database boot: ensure paths, start hiqlite, migrate.
//!
//! Runtime uses hiqlite (on-disk SQLite state machine). Unit tests apply the
//! same migration SQL against an in-memory `rusqlite` connection.

use std::fs::{self, File};
use std::time::Instant;

use hiqlite::macros::CacheVariants;
use hiqlite::{Client, Error as HiqliteError, Node, NodeConfig, start_node_with_cache};
use tracing::{info, info_span, instrument, warn};

use crate::auth::Migrations;
use crate::operations::config::{AppConfig, Mode};

/// Cache index for the session (and future) hot paths.
#[derive(Debug, CacheVariants)]
pub enum AppCache {
    Sessions,
}

/// Ensure the database directory and file exist, start hiqlite, run migrations.
#[instrument(name = "db.open_and_migrate", skip(cfg), fields(db_path = %cfg.db_path.display(), mode = cfg.mode.as_str()))]
pub async fn open_and_migrate(cfg: &AppConfig) -> Result<Client, DbError> {
    let boot = Instant::now();

    ensure_db_paths(cfg)?;

    let node_config = node_config_for(cfg)?;
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

    {
        let span = info_span!("db.migrate");
        let _guard = span.enter();
        let started = Instant::now();
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

/// Create `db_path`'s parent directory and the file itself if missing.
#[instrument(name = "db.ensure_paths", skip(cfg), fields(db_path = %cfg.db_path.display()))]
pub fn ensure_db_paths(cfg: &AppConfig) -> Result<(), DbError> {
    let started = Instant::now();
    let dir = cfg.db_dir();
    if !dir.exists() {
        info!(path = %dir.display(), "creating database directory");
        fs::create_dir_all(&dir).map_err(|e| DbError::Io {
            path: dir.display().to_string(),
            source: e,
        })?;
    }

    if !cfg.db_path.exists() {
        info!(path = %cfg.db_path.display(), "creating database file");
        File::create(&cfg.db_path).map_err(|e| DbError::Io {
            path: cfg.db_path.display().to_string(),
            source: e,
        })?;
    }

    info!(
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "database paths ensured"
    );
    Ok(())
}

fn node_config_for(cfg: &AppConfig) -> Result<NodeConfig, DbError> {
    let data_dir = cfg.db_dir();
    let filename_db = cfg.db_filename();

    // Single-node embedded defaults so `cargo run` needs no extra secrets.
    // Prod must supply real raft secrets via env before we expand the fleet.
    let (secret_raft, secret_api) = match cfg.mode {
        Mode::Dev => (
            "rn-site-dev-raft-secret".to_string(),
            "rn-site-dev-api-secret".to_string(),
        ),
        Mode::Prod => {
            let secret_raft = std::env::var("RN_SITE_HQL_SECRET_RAFT").map_err(|_| {
                DbError::Config(
                    "mode=prod requires RN_SITE_HQL_SECRET_RAFT (≥16 chars)".into(),
                )
            })?;
            let secret_api = std::env::var("RN_SITE_HQL_SECRET_API").map_err(|_| {
                DbError::Config("mode=prod requires RN_SITE_HQL_SECRET_API (≥16 chars)".into())
            })?;
            if secret_raft.len() < 16 || secret_api.len() < 16 {
                return Err(DbError::Config(
                    "raft/api secrets must be at least 16 characters".into(),
                ));
            }
            (secret_raft, secret_api)
        }
    };

    if cfg.mode == Mode::Dev {
        warn!("hiqlite using built-in dev secrets; not for production");
    }

    let raft_addr = std::env::var("RN_SITE_HQL_ADDR_RAFT")
        .unwrap_or_else(|_| "127.0.0.1:8100".to_string());
    let api_addr =
        std::env::var("RN_SITE_HQL_ADDR_API").unwrap_or_else(|_| "127.0.0.1:8200".to_string());

    Ok(NodeConfig {
        node_id: 1,
        nodes: vec![Node {
            id: 1,
            addr_raft: raft_addr,
            addr_api: api_addr,
        }],
        data_dir: data_dir.to_string_lossy().into_owned().into(),
        filename_db: filename_db.into(),
        secret_raft,
        secret_api,
        health_check_delay_secs: 0,
        ..NodeConfig::default()
    })
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
    #[error(transparent)]
    Hiqlite(#[from] HiqliteError),
}

/// Apply embedded migrations to an in-memory SQLite connection (tests).
#[instrument(name = "db.open_memory_migrated")]
pub fn open_memory_migrated() -> Result<rusqlite::Connection, rusqlite::Error> {
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
fn apply_migrations_rusqlite(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            ts INTEGER NOT NULL,
            hash TEXT NOT NULL
        );",
    )?;

    let mut files: Vec<_> = Migrations::iter().collect();
    files.sort();

    for name in files {
        let (id_str, rest) = name
            .split_once('_')
            .expect("migration file names are `<id>_<name>.sql`");
        let id: i64 = id_str.parse().expect("migration id is an integer");
        let migration_name = rest.strip_suffix(".sql").unwrap_or(rest);

        let already: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE id = ?1)",
            [id],
            |row| row.get(0),
        )?;
        if already {
            continue;
        }

        let file = Migrations::get(name.as_ref()).expect("embedded migration");
        let sql = std::str::from_utf8(file.data.as_ref()).expect("migration is utf-8");
        let started = Instant::now();
        conn.execute_batch(sql)?;
        info!(
            migration = %name,
            latency_ms = started.elapsed().as_secs_f64() * 1000.0,
            "applied migration"
        );

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after 1970")
            .as_secs() as i64;
        let hash = hex::encode(file.metadata.sha256_hash());
        conn.execute(
            "INSERT INTO _migrations (id, name, ts, hash) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, migration_name, ts, hash],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
