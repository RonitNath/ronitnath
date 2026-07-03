//! Queries and types for the `audit_log` table.
//!
//! One row per attributable mutation, plus pre-auth security events. Call
//! [`Store::audit`] from every exemplar mutation so forks copy the habit.

use serde::Serialize;
use ts_rs::TS;

use super::Store;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AuditEntry {
    pub id: i64,
    pub at: String,
    pub identity_display_name: Option<String>,
    pub action: String,
    pub entity: String,
    pub entity_id: Option<String>,
    pub detail: String,
}

impl Store {
    #[allow(clippy::too_many_arguments)]
    pub async fn audit(
        &self,
        identity_id: Option<i64>,
        account_id: Option<i64>,
        request_id: Option<&str>,
        action: &str,
        entity: &str,
        entity_id: Option<&str>,
        detail: &serde_json::Value,
    ) -> sqlx::Result<()> {
        let detail = detail.to_string();
        sqlx::query!(
            r#"INSERT INTO audit_log (identity_id, account_id, request_id, action, entity, entity_id, detail)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            identity_id,
            account_id,
            request_id,
            action,
            entity,
            entity_id,
            detail,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_audit_log(&self, account_id: i64, limit: i64) -> sqlx::Result<Vec<AuditEntry>> {
        sqlx::query_as!(
            AuditEntry,
            r#"SELECT al.id as "id: i64", al.at,
                      i.display_name as identity_display_name,
                      al.action, al.entity, al.entity_id, al.detail
               FROM audit_log al
               LEFT JOIN identities i ON i.id = al.identity_id
               WHERE al.account_id = ?1
               ORDER BY al.id DESC
               LIMIT ?2"#,
            account_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Pre-auth events (`login.failed`) have no `account_id` to scope by —
    /// only used to assert one got logged in tests.
    #[cfg(test)]
    pub async fn count_audit_events(&self, action: &str) -> sqlx::Result<i64> {
        let row = sqlx::query!("SELECT COUNT(*) as count FROM audit_log WHERE action = ?1", action)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.count)
    }
}
