//! Persistence boundary.
//!
//! [`Store`] wraps a `SqlitePool` and owns every query in the app (organized
//! one file per table). Handlers call `Store` methods and
//! never see SQL or a pool directly — if this ever needs to move to Postgres,
//! this module is the seam: swap the pool type and query bodies, keep the
//! method signatures.

pub mod accounts;
pub mod attendance;
pub mod audience;
pub mod audit;
pub mod calendar_entries;
pub mod calendar_feed_tokens;
pub mod circles;
pub mod event_links;
pub mod events;
pub mod factors;
pub mod identities;
pub mod import;
pub mod memberships;
pub mod people;
pub mod person_identity_links;
pub mod photos;
pub mod schedule_items;
pub mod segment_rsvps;
pub mod sessions;

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

// Deploy preflight owns divergence refusal; boot tolerates DB-ahead so rollback
// always boots (the N-1 rule).
static MIGRATOR: sqlx::migrate::Migrator = {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.ignore_missing = true;
    migrator
};

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

    /// Opens an existing database without applying migrations. The admin
    /// bin uses this so schema changes are applied by `site` first.
    pub async fn connect_existing(database_url: &str) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(false);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        let expected = MIGRATOR
            .iter()
            .map(|migration| migration.version)
            .max()
            .ok_or_else(|| anyhow::anyhow!("no embedded migrations"))?;
        let applied =
            sqlx::query_scalar!(r#"SELECT MAX(version) as "version?: i64" FROM _sqlx_migrations"#)
                .fetch_optional(&pool)
                .await?
                .flatten();
        match applied {
            Some(applied) if applied < expected => anyhow::bail!(
                "database migration version {applied} is older than this binary expects ({expected}); start site first"
            ),
            // Newer-than-embedded is the N-1 rollback state; migrations must be
            // tolerable by the previous binary.
            Some(_) => Ok(Self { pool }),
            None => anyhow::bail!("database has no applied migrations; start site first"),
        }
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

    /// Creates identity + personal account + owner membership + password
    /// factor atomically — the one signup transaction every identity goes
    /// through (see `docs/plans/2026-07-stage2-hardened-fork-template.md`).
    /// `email` should already be lowercased/trimmed by the caller, since
    /// it's the uniqueness key on `factors`.
    pub async fn signup_with_password(
        &self,
        display_name: &str,
        email: &str,
        password_hash: &str,
    ) -> sqlx::Result<(i64, i64)> {
        let mut tx = self.pool.begin().await?;

        let identity_id = sqlx::query_scalar!(
            r#"INSERT INTO identities (kind, display_name) VALUES ('human', ?1) RETURNING id as "id!: i64""#,
            display_name,
        )
        .fetch_one(&mut *tx)
        .await?;

        let account_id = sqlx::query_scalar!(
            r#"INSERT INTO accounts (name, kind) VALUES (?1, 'personal') RETURNING id as "id!: i64""#,
            display_name,
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            "INSERT INTO memberships (identity_id, account_id, role) VALUES (?1, ?2, 'owner')",
            identity_id,
            account_id,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            "INSERT INTO factors (identity_id, kind, external_id, secret_hash) VALUES (?1, 'password', ?2, ?3)",
            identity_id,
            email,
            password_hash,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((identity_id, account_id))
    }
}

#[cfg(test)]
mod tests {
    use super::{MIGRATOR, Store};

    fn scratch_database(label: &str) -> (std::path::PathBuf, String) {
        let path = std::env::temp_dir().join(format!(
            "ronitnath-{label}-{}.db",
            crate::auth::session::generate_token()
        ));
        let url = format!("sqlite:{}", path.display());
        (path, url)
    }

    fn expected_version() -> i64 {
        MIGRATOR
            .iter()
            .map(|migration| migration.version)
            .max()
            .expect("embedded migrations")
    }

    async fn insert_future_migration(store: &Store) {
        sqlx::query(
            "INSERT INTO _sqlx_migrations \
             (version, description, success, checksum, execution_time) \
             VALUES (?1, 'future migration', TRUE, X'', 0)",
        )
        .bind(expected_version() + 1)
        .execute(&store.pool)
        .await
        .expect("insert future migration row");
    }

    #[tokio::test]
    async fn connect_existing_accepts_database_ahead_of_binary() {
        let (path, url) = scratch_database("existing-ahead");
        let store = Store::connect(&url)
            .await
            .expect("migrate scratch database");
        insert_future_migration(&store).await;
        store.pool.close().await;

        let reopened = Store::connect_existing(&url)
            .await
            .expect("connect_existing should accept a DB-ahead schema");
        reopened.pool.close().await;
        std::fs::remove_file(path).expect("remove scratch database");
    }

    #[tokio::test]
    async fn connect_ignores_unknown_applied_migration() {
        let (path, url) = scratch_database("connect-ahead");
        let store = Store::connect(&url)
            .await
            .expect("migrate scratch database");
        insert_future_migration(&store).await;
        store.pool.close().await;

        let reopened = Store::connect(&url)
            .await
            .expect("connect should ignore an unknown applied migration");
        reopened.pool.close().await;
        std::fs::remove_file(path).expect("remove scratch database");
    }

    #[tokio::test]
    async fn connect_still_validates_embedded_migration_checksums() {
        let (path, url) = scratch_database("checksum");
        let store = Store::connect(&url)
            .await
            .expect("migrate scratch database");
        sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = ?1")
            .bind(expected_version())
            .execute(&store.pool)
            .await
            .expect("corrupt embedded migration checksum");
        store.pool.close().await;

        let error = match Store::connect(&url).await {
            Ok(store) => {
                store.pool.close().await;
                panic!("connect should reject an embedded migration checksum mismatch")
            }
            Err(error) => error,
        };
        let message = error.to_string();
        // A failed pool open can release its Windows file handle asynchronously.
        let _ = std::fs::remove_file(path);
        assert!(
            message.contains("previously applied but has been modified"),
            "unexpected error: {error:#}"
        );
    }
}
