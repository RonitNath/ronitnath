//! Revocable, person-bound calendar feed capabilities.

use super::Store;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CalendarFeedToken {
    pub id: i64,
    pub account_id: i64,
    pub person_id: i64,
    pub token_plain: String,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

impl Store {
    pub async fn find_calendar_feed_for_person(
        &self,
        account_id: i64,
        person_id: i64,
    ) -> sqlx::Result<Option<CalendarFeedToken>> {
        sqlx::query_as!(
            CalendarFeedToken,
            r#"SELECT id as "id!: i64", account_id as "account_id!: i64",
            person_id as "person_id!: i64", token_plain, revoked_at, last_used_at, created_at
            FROM calendar_feed_tokens WHERE account_id=?1 AND person_id=?2"#,
            account_id,
            person_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn resolve_calendar_feed(
        &self,
        token_hash: &str,
    ) -> sqlx::Result<Option<CalendarFeedToken>> {
        sqlx::query_as!(
            CalendarFeedToken,
            r#"SELECT id as "id!: i64", account_id as "account_id!: i64",
            person_id as "person_id!: i64", token_plain, revoked_at, last_used_at, created_at
            FROM calendar_feed_tokens WHERE token_hash=?1 AND revoked_at IS NULL"#,
            token_hash
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn mint_calendar_feed(
        &self,
        account_id: i64,
        person_id: i64,
        token_hash: &str,
        token_plain: &str,
    ) -> sqlx::Result<CalendarFeedToken> {
        sqlx::query_as!(CalendarFeedToken, r#"INSERT INTO calendar_feed_tokens
            (account_id, person_id, token_hash, token_plain) SELECT ?1, p.id, ?3, ?4
            FROM people p WHERE p.account_id=?1 AND p.id=?2
            ON CONFLICT(account_id, person_id) DO UPDATE SET token_hash=excluded.token_hash,
                token_plain=excluded.token_plain, revoked_at=NULL, last_used_at=NULL, created_at=datetime('now')
            RETURNING id as "id!: i64", account_id as "account_id!: i64", person_id as "person_id!: i64",
                token_plain, revoked_at, last_used_at, created_at"#, account_id, person_id, token_hash, token_plain)
            .fetch_one(&self.pool).await
    }

    /// Marks a feed use only while it is still live. A zero result means a
    /// revoke won the race after token resolution and callers must fail closed.
    pub async fn touch_calendar_feed(&self, id: i64) -> sqlx::Result<u64> {
        Ok(sqlx::query!("UPDATE calendar_feed_tokens SET last_used_at=datetime('now') WHERE id=?1 AND revoked_at IS NULL", id)
            .execute(&self.pool).await?.rows_affected())
    }

    pub async fn revoke_calendar_feed(&self, account_id: i64, person_id: i64) -> sqlx::Result<u64> {
        Ok(sqlx::query!("UPDATE calendar_feed_tokens SET revoked_at=datetime('now') WHERE account_id=?1 AND person_id=?2 AND revoked_at IS NULL", account_id, person_id)
            .execute(&self.pool).await?.rows_affected())
    }
}
