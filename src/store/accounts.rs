//! Queries and types for the `accounts` table.
//!
//! An account is the unit of legal ownership — every domain table (e.g.
//! `guestbook_entries`) FKs to one, never directly to an identity.

use super::Store;

impl Store {
    pub async fn rename_account(&self, id: i64, name: &str) -> sqlx::Result<()> {
        sqlx::query!("UPDATE accounts SET name = ?1 WHERE id = ?2", name, id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
