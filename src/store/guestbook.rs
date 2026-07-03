//! Queries and types for the `guestbook_entries` table.
//!
//! The demo vertical slice: table → [`Store`] methods here → JSON handlers in
//! [`crate::handlers::guestbook`] → a Solid island in `ts/src/islands`.
//!
//! Account-scoped exemplar: every query takes an `account_id` and every
//! write stamps one, so this is the pattern to copy for new domain tables —
//! see [`crate::auth::AccountScope`], which is what handlers use to get one
//! instead of trusting a raw id from the request.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utoipa::ToSchema;

use super::Store;

#[derive(Debug, Serialize, sqlx::FromRow, TS, ToSchema)]
#[ts(export)]
pub struct GuestbookEntry {
    pub id: i32,
    pub author: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct NewGuestbookEntry {
    pub author: String,
    pub message: String,
}

impl Store {
    pub async fn list_guestbook(&self, account_id: i64) -> sqlx::Result<Vec<GuestbookEntry>> {
        sqlx::query_as!(
            GuestbookEntry,
            r#"SELECT id as "id: i32", author, message, created_at
               FROM guestbook_entries WHERE account_id = ?1 ORDER BY id"#,
            account_id,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn add_guestbook_entry(
        &self,
        account_id: i64,
        entry: NewGuestbookEntry,
    ) -> sqlx::Result<GuestbookEntry> {
        sqlx::query_as!(
            GuestbookEntry,
            r#"INSERT INTO guestbook_entries (account_id, author, message)
               VALUES (?1, ?2, ?3)
               RETURNING id as "id!: i32", author, message, created_at"#,
            account_id,
            entry.author,
            entry.message,
        )
        .fetch_one(&self.pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip_is_scoped_per_account() {
        let store = Store::connect_in_memory().await;
        let (_, account_a) = store
            .signup_with_password("Alice", "alice@example.com", "hash-a")
            .await
            .unwrap();
        let (_, account_b) = store
            .signup_with_password("Bob", "bob@example.com", "hash-b")
            .await
            .unwrap();

        assert_eq!(store.list_guestbook(account_a).await.unwrap().len(), 0);

        let created = store
            .add_guestbook_entry(
                account_a,
                NewGuestbookEntry {
                    author: "test".into(),
                    message: "hello".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(created.author, "test");

        assert_eq!(store.list_guestbook(account_a).await.unwrap().len(), 1);
        assert_eq!(
            store.list_guestbook(account_b).await.unwrap().len(),
            0,
            "account B must not see account A's entries"
        );
    }
}
