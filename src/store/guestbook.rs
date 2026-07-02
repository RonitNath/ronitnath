//! Queries and types for the `guestbook_entries` table.
//!
//! The demo vertical slice: table → [`Store`] methods here → JSON handlers in
//! [`crate::handlers::guestbook`] → a Solid island in `ts/src/islands`.

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
    pub async fn list_guestbook(&self) -> sqlx::Result<Vec<GuestbookEntry>> {
        sqlx::query_as!(
            GuestbookEntry,
            r#"SELECT id as "id: i32", author, message, created_at
               FROM guestbook_entries ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn add_guestbook_entry(
        &self,
        entry: NewGuestbookEntry,
    ) -> sqlx::Result<GuestbookEntry> {
        sqlx::query_as!(
            GuestbookEntry,
            r#"INSERT INTO guestbook_entries (author, message)
               VALUES (?1, ?2)
               RETURNING id as "id: i32", author, message, created_at"#,
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
    async fn roundtrip() {
        let store = Store::connect_in_memory().await;

        let seeded = store.list_guestbook().await.unwrap();
        assert_eq!(seeded.len(), 1, "seed migration should insert one entry");

        let created = store
            .add_guestbook_entry(NewGuestbookEntry {
                author: "test".into(),
                message: "hello".into(),
            })
            .await
            .unwrap();
        assert_eq!(created.author, "test");
        assert_eq!(created.message, "hello");

        let all = store.list_guestbook().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].id, created.id);
    }
}
