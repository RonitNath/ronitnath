//! Queries and types for the `factors` table.
//!
//! A factor is a pluggable login mechanism attached to an identity — see
//! [`crate::auth`] for the `FactorKind` trait built-in kinds (`password`,
//! `api_token`) implement.

use serde::Serialize;
use ts_rs::TS;

use super::Store;

#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
#[ts(export)]
pub struct Factor {
    pub id: i64,
    pub identity_id: i64,
    pub kind: String,
    pub external_id: Option<String>,
    #[serde(skip)]
    #[ts(skip)]
    pub secret_hash: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

impl Store {
    pub async fn find_factor_by_external(
        &self,
        kind: &str,
        external_id: &str,
    ) -> sqlx::Result<Option<Factor>> {
        sqlx::query_as!(
            Factor,
            r#"SELECT id as "id: i64", identity_id as "identity_id: i64", kind,
                      external_id, secret_hash, created_at, last_used_at
               FROM factors WHERE kind = ?1 AND external_id = ?2"#,
            kind,
            external_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    /// Looks up an `api_token` factor by the sha256 of the raw bearer token.
    pub async fn find_factor_by_secret_hash(&self, secret_hash: &str) -> sqlx::Result<Option<Factor>> {
        sqlx::query_as!(
            Factor,
            r#"SELECT id as "id: i64", identity_id as "identity_id: i64", kind,
                      external_id, secret_hash, created_at, last_used_at
               FROM factors WHERE kind = 'api_token' AND secret_hash = ?1"#,
            secret_hash,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_factors(&self, identity_id: i64) -> sqlx::Result<Vec<Factor>> {
        sqlx::query_as!(
            Factor,
            r#"SELECT id as "id: i64", identity_id as "identity_id: i64", kind,
                      external_id, secret_hash, created_at, last_used_at
               FROM factors WHERE identity_id = ?1 ORDER BY id"#,
            identity_id,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn count_factors(&self, identity_id: i64) -> sqlx::Result<i64> {
        let row = sqlx::query!(
            "SELECT COUNT(*) as count FROM factors WHERE identity_id = ?1",
            identity_id,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.count)
    }

    pub async fn create_factor(
        &self,
        identity_id: i64,
        kind: &str,
        external_id: Option<&str>,
        secret_hash: Option<&str>,
    ) -> sqlx::Result<Factor> {
        sqlx::query_as!(
            Factor,
            r#"INSERT INTO factors (identity_id, kind, external_id, secret_hash)
               VALUES (?1, ?2, ?3, ?4)
               RETURNING id as "id!: i64", identity_id as "identity_id!: i64", kind,
                         external_id, secret_hash, created_at, last_used_at"#,
            identity_id,
            kind,
            external_id,
            secret_hash,
        )
        .fetch_one(&self.pool)
        .await
    }

    /// Deletes a factor owned by `identity_id` (the identity check prevents
    /// removing someone else's factor by guessing an id). Callers must check
    /// [`Store::count_factors`] first — this alone doesn't stop an identity
    /// from removing its last factor.
    pub async fn delete_factor(&self, id: i64, identity_id: i64) -> sqlx::Result<()> {
        sqlx::query!(
            "DELETE FROM factors WHERE id = ?1 AND identity_id = ?2",
            id,
            identity_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn touch_factor_last_used(&self, id: i64) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE factors SET last_used_at = datetime('now') WHERE id = ?1",
            id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
