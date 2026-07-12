//! Queries and types for the `accounts` table.
//!
//! An account is the unit of legal ownership — every domain table (e.g.
//! `guestbook_entries`) FKs to one, never directly to an identity.

use super::Store;

impl Store {
    /// Resolves the platform owner's account explicitly by purpose. Fails
    /// closed if corrupt/legacy data has more than one primary account.
    pub async fn find_primary_account(&self) -> anyhow::Result<Option<i64>> {
        let rows = sqlx::query!(
            r#"SELECT id as "id!: i64" FROM accounts WHERE purpose = 'primary' ORDER BY id LIMIT 2"#
        )
        .fetch_all(&self.pool)
        .await?;
        match rows.as_slice() {
            [row] => Ok(Some(row.id)),
            [] => Ok(None),
            _ => anyhow::bail!("multiple primary accounts exist; account purpose is ambiguous"),
        }
    }

    pub async fn require_primary_account(&self) -> anyhow::Result<i64> {
        self.find_primary_account().await?.ok_or_else(|| {
            anyhow::anyhow!("no primary account exists; sign up in the admin server first")
        })
    }

    pub async fn rename_account(&self, id: i64, name: &str) -> sqlx::Result<()> {
        sqlx::query!("UPDATE accounts SET name = ?1 WHERE id = ?2", name, id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
