//! Queries and types for the `factors` table.
//!
//! A factor is a pluggable login mechanism attached to an identity — see
//! [`crate::auth`] for the `FactorKind` trait built-in kinds (`password`,
//! `api_token`) implement.

use serde::Serialize;
use ts_rs::TS;

use crate::auth::oidc::PendingOidcState;
use crate::auth::session;

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

pub struct PendingOidcAuth {
    pub identity_id: Option<i64>,
    pub account_id: Option<i64>,
    pub state: PendingOidcState,
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
    pub async fn find_factor_by_secret_hash(
        &self,
        secret_hash: &str,
    ) -> sqlx::Result<Option<Factor>> {
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

    pub async fn create_oidc_factor(
        &self,
        identity_id: i64,
        external_id: &str,
        metadata: &serde_json::Value,
    ) -> sqlx::Result<Factor> {
        let metadata = metadata.to_string();
        sqlx::query_as!(
            Factor,
            r#"INSERT INTO factors (identity_id, kind, external_id, metadata)
               VALUES (?1, 'oidc', ?2, ?3)
               RETURNING id as "id!: i64", identity_id as "identity_id!: i64", kind,
                         external_id, secret_hash, created_at, last_used_at"#,
            identity_id,
            external_id,
            metadata,
        )
        .fetch_one(&self.pool)
        .await
    }

    pub async fn signup_with_oidc(
        &self,
        display_name: &str,
        external_id: &str,
        email: Option<&str>,
    ) -> sqlx::Result<(i64, i64, i64)> {
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
        let metadata = serde_json::json!({ "email": email }).to_string();
        let factor_id = sqlx::query_scalar!(
            r#"INSERT INTO factors (identity_id, kind, external_id, metadata)
               VALUES (?1, 'oidc', ?2, ?3)
               RETURNING id as "id!: i64""#,
            identity_id,
            external_id,
            metadata,
        )
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok((identity_id, account_id, factor_id))
    }

    pub async fn create_pending_oidc(
        &self,
        raw_state: &str,
        identity_id: Option<i64>,
        account_id: Option<i64>,
        pending: &PendingOidcState,
    ) -> sqlx::Result<()> {
        sqlx::query!("DELETE FROM pending_auth WHERE expires_at <= datetime('now') OR consumed_at IS NOT NULL")
            .execute(&self.pool)
            .await?;
        let token_hash = session::hash_token(raw_state);
        let state =
            serde_json::to_string(pending).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        let expires_at = (time::OffsetDateTime::now_utc() + time::Duration::minutes(10))
            .format(&time::macros::format_description!(
                "[year]-[month]-[day] [hour]:[minute]:[second]"
            ))
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        sqlx::query!(
            r#"INSERT INTO pending_auth (kind, token_hash, factor_kind, state, identity_id, account_id, expires_at)
               VALUES ('oidc', ?1, 'oidc', ?2, ?3, ?4, ?5)"#,
            token_hash,
            state,
            identity_id,
            account_id,
            expires_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn consume_pending_oidc(&self, raw_state: &str) -> sqlx::Result<PendingOidcAuth> {
        let token_hash = session::hash_token(raw_state);
        let row = sqlx::query!(
            r#"UPDATE pending_auth
               SET consumed_at = datetime('now')
               WHERE token_hash = ?1 AND kind = 'oidc' AND factor_kind = 'oidc'
                 AND consumed_at IS NULL AND expires_at > datetime('now')
               RETURNING identity_id as "identity_id?: i64", account_id as "account_id?: i64", state"#,
            token_hash,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
        let state =
            serde_json::from_str(&row.state).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        Ok(PendingOidcAuth {
            identity_id: row.identity_id,
            account_id: row.account_id,
            state,
        })
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
