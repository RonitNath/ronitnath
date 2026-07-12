//! Queries and types for the `accounts` table.
//!
//! An account is the unit of legal ownership — every domain table (e.g.
//! `guestbook_entries`) FKs to one, never directly to an identity.

use super::Store;

impl Store {
    /// CLI convenience for this single-owner deployment. Fails closed when
    /// account selection would be ambiguous.
    pub async fn require_single_account(&self) -> anyhow::Result<i64> {
        let rows = sqlx::query!(r#"SELECT id as "id!: i64" FROM accounts ORDER BY id LIMIT 2"#)
            .fetch_all(&self.pool)
            .await?;
        match rows.as_slice() {
            [row] => Ok(row.id),
            [] => anyhow::bail!("no accounts exist; sign up in the admin server first"),
            _ => anyhow::bail!("multiple accounts exist; CLI requires an explicit account selector"),
        }
    }

    pub async fn rename_account(&self, id: i64, name: &str) -> sqlx::Result<()> {
        sqlx::query!("UPDATE accounts SET name = ?1 WHERE id = ?2", name, id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
