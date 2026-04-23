use std::str::FromStr;

use anyhow::Context as _;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Executor as _, SqlitePool};

pub(crate) async fn open_pool(database_url: &str) -> anyhow::Result<SqlitePool> {
    ensure_sqlite_parent(database_url)?;
    let options = SqliteConnectOptions::from_str(database_url)
        .with_context(|| format!("invalid database_url={database_url}"))?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .context("open sqlite pool")?;
    pool.execute("PRAGMA foreign_keys = ON")
        .await
        .context("enable sqlite foreign keys")?;
    Ok(pool)
}

pub(crate) async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("run database migrations")
}

fn ensure_sqlite_parent(database_url: &str) -> anyhow::Result<()> {
    let Some(path) = database_url.strip_prefix("sqlite://") else {
        return Ok(());
    };
    if path == ":memory:" || path.starts_with("file:") {
        return Ok(());
    }
    let path = std::path::Path::new(path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).context("create sqlite database directory")?;
    }
    Ok(())
}
