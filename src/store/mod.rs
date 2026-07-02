//! Persistence boundary.
//!
//! [`Store`] wraps a `SqlitePool` and owns every query in the app (organized
//! one file per table, e.g. [`guestbook`]). Handlers call `Store` methods and
//! never see SQL or a pool directly — if this ever needs to move to Postgres,
//! this module is the seam: swap the pool type and query bodies, keep the
//! method signatures.

pub mod guestbook;

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Connects to the sqlite database at `database_url`, creating the file
    /// (and any missing parent directory) if it doesn't exist yet, then runs
    /// pending migrations. A fresh fork needs no setup beyond `cargo run`.
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        if let Some(path) = SqliteConnectOptions::from_str(database_url)?
            .get_filename()
            .parent()
        {
            std::fs::create_dir_all(path)?;
        }

        let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;

        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    /// An in-memory, migrated database for tests. Each call gets its own
    /// isolated instance, so tests can run in parallel (validation.md).
    #[cfg(test)]
    pub async fn connect_in_memory() -> Self {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite connection");
        let store = Self { pool };
        store.migrate().await.expect("run migrations");
        store
    }

    async fn migrate(&self) -> anyhow::Result<()> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }
}
