//! Queries and types for the `schedule_items` table.

use serde::Serialize;
use ts_rs::TS;
use utoipa::ToSchema;

use super::Store;
use crate::access::level::Level;

#[derive(Debug, Serialize, sqlx::FromRow, TS, ToSchema)]
#[ts(export)]
pub struct ScheduleItem {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub sort_order: i64,
    pub time_label: String,
    pub title: String,
    pub detail: String,
    pub tier: String,
    pub segment_key: Option<String>,
}

pub struct ScheduleItemFields {
    pub sort_order: i64,
    pub time_label: String,
    pub title: String,
    pub detail: String,
    pub tier: String,
    pub segment_key: Option<String>,
}

impl Store {
    /// Schedule redaction chokepoint: Busy/Hidden reveal no item titles;
    /// Summary reveals public items; private items require Full.
    pub async fn list_schedule(
        &self,
        account_id: i64,
        event_id: i64,
        level: Level,
    ) -> sqlx::Result<Vec<ScheduleItem>> {
        let level = level as i64;
        sqlx::query_as!(
            ScheduleItem,
            r#"SELECT id as "id!: i64", sort_order as "sort_order!: i64", time_label, title,
                      detail, tier, segment_key
               FROM schedule_items
               WHERE account_id = ?1 AND event_id = ?2
                 AND ?3 >= 2 AND (tier = 'public' OR ?3 = 3)
               ORDER BY sort_order, id"#,
            account_id,
            event_id,
            level,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn create_schedule_item(
        &self,
        account_id: i64,
        event_id: i64,
        fields: &ScheduleItemFields,
    ) -> sqlx::Result<i64> {
        sqlx::query_scalar!(
            r#"INSERT INTO schedule_items
                   (account_id, event_id, sort_order, time_label, title, detail, tier, segment_key)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
               RETURNING id as "id!: i64""#,
            account_id,
            event_id,
            fields.sort_order,
            fields.time_label,
            fields.title,
            fields.detail,
            fields.tier,
            fields.segment_key,
        )
        .fetch_one(&self.pool)
        .await
    }

    pub async fn update_schedule_item(
        &self,
        account_id: i64,
        item_id: i64,
        fields: &ScheduleItemFields,
    ) -> sqlx::Result<u64> {
        let result = sqlx::query!(
            r#"UPDATE schedule_items
               SET sort_order = ?3, time_label = ?4, title = ?5, detail = ?6,
                   tier = ?7, segment_key = ?8
               WHERE account_id = ?1 AND id = ?2"#,
            account_id,
            item_id,
            fields.sort_order,
            fields.time_label,
            fields.title,
            fields.detail,
            fields.tier,
            fields.segment_key,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_schedule_item(&self, account_id: i64, item_id: i64) -> sqlx::Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM schedule_items WHERE account_id = ?1 AND id = ?2",
            account_id,
            item_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
