//! Queries and types for the `identities` table.
//!
//! An identity is the acting entity (human, agent, or service) — see
//! `docs/plans/2026-07-stage2-hardened-fork-template.md` for the full model.

use serde::Serialize;
use ts_rs::TS;

use super::Store;

#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
#[ts(export)]
pub struct Identity {
    pub id: i64,
    pub kind: String,
    pub display_name: String,
    pub created_at: String,
}

impl Store {
    pub async fn find_identity(&self, id: i64) -> sqlx::Result<Option<Identity>> {
        sqlx::query_as!(
            Identity,
            r#"SELECT id as "id: i64", kind, display_name, created_at
               FROM identities WHERE id = ?1 AND deleted_at IS NULL"#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }
}
