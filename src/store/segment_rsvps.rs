//! Queries and types for the `segment_rsvps` table (person × schedule
//! segment) — how a long day (board games → dinner → fireworks → rooftop →
//! sleepover) gets managed per person instead of as one yes/no.

use serde::Serialize;
use ts_rs::TS;
use utoipa::ToSchema;

use super::Store;

#[derive(Debug, Serialize, sqlx::FromRow, TS, ToSchema)]
#[ts(export)]
pub struct SegmentRsvp {
    #[ts(type = "number")]
    pub schedule_item_id: i64,
    pub status: String,
}

/// Per-segment tallies for in/maybe displays ("Sleepover: 5 in").
#[derive(Debug, Serialize, sqlx::FromRow, TS, ToSchema)]
#[ts(export)]
pub struct SegmentCount {
    #[ts(type = "number")]
    pub schedule_item_id: i64,
    #[ts(type = "number")]
    pub in_count: i64,
    #[ts(type = "number")]
    pub maybe_count: i64,
}

impl Store {
    pub async fn upsert_segment_rsvp(
        &self,
        account_id: i64,
        schedule_item_id: i64,
        person_id: i64,
        status: &str,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"INSERT INTO segment_rsvps (account_id, schedule_item_id, person_id, status)
               VALUES (?1, ?2, ?3, ?4)
               ON CONFLICT (schedule_item_id, person_id) DO UPDATE SET
                   status = excluded.status,
                   updated_at = datetime('now')"#,
            account_id,
            schedule_item_id,
            person_id,
            status,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Admin-set bookkeeping on a person's segment row: whether they paid
    /// (dinner) and whether they actually showed up. `None` leaves a flag
    /// untouched. Creates the row (status `in`) if the person never marked
    /// the segment themselves — recording a payment implies they're in.
    pub async fn set_segment_flags(
        &self,
        account_id: i64,
        event_id: i64,
        segment_key: &str,
        person_id: i64,
        status: Option<&str>,
        paid: Option<bool>,
        attended: Option<bool>,
    ) -> sqlx::Result<u64> {
        let paid = paid.map(i64::from);
        let attended = attended.map(i64::from);
        let result = sqlx::query!(
            r#"INSERT INTO segment_rsvps (account_id, schedule_item_id, person_id, status, paid, attended)
               SELECT si.account_id, si.id, ?4, COALESCE(?5, 'in'), COALESCE(?6, 0), ?7
               FROM schedule_items si
               WHERE si.account_id = ?1 AND si.event_id = ?2 AND si.segment_key = ?3
               ON CONFLICT (schedule_item_id, person_id) DO UPDATE SET
                   status = COALESCE(?5, status),
                   paid = COALESCE(?6, paid),
                   attended = COALESCE(?7, attended),
                   updated_at = datetime('now')"#,
            account_id,
            event_id,
            segment_key,
            person_id,
            status,
            paid,
            attended,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn list_segment_rsvps_for_person(
        &self,
        account_id: i64,
        event_id: i64,
        person_id: i64,
    ) -> sqlx::Result<Vec<SegmentRsvp>> {
        sqlx::query_as!(
            SegmentRsvp,
            r#"SELECT sr.schedule_item_id as "schedule_item_id!: i64", sr.status
               FROM segment_rsvps sr
               JOIN schedule_items si ON si.id = sr.schedule_item_id
               WHERE sr.account_id = ?1 AND si.event_id = ?2 AND sr.person_id = ?3"#,
            account_id,
            event_id,
            person_id,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn segment_counts(
        &self,
        account_id: i64,
        event_id: i64,
    ) -> sqlx::Result<Vec<SegmentCount>> {
        sqlx::query_as!(
            SegmentCount,
            r#"SELECT si.id as "schedule_item_id!: i64",
                      COALESCE(SUM(CASE WHEN sr.status = 'in' THEN 1 ELSE 0 END), 0) as "in_count!: i64",
                      COALESCE(SUM(CASE WHEN sr.status = 'maybe' THEN 1 ELSE 0 END), 0) as "maybe_count!: i64"
               FROM schedule_items si
               LEFT JOIN segment_rsvps sr
                 ON sr.account_id = si.account_id AND sr.schedule_item_id = si.id
               WHERE si.account_id = ?1 AND si.event_id = ?2 AND si.segment_key IS NOT NULL
               GROUP BY si.id"#,
            account_id,
            event_id,
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Live count of guests marked `in` for a named segment (e.g.
    /// `sleepover`) — for the public landing page, unscoped like
    /// `Store::public_attendee_count`.
    pub async fn public_segment_in_count(&self, event_id: i64, segment_key: &str) -> sqlx::Result<i64> {
        let row = sqlx::query!(
            r#"SELECT COALESCE(SUM(CASE WHEN sr.status = 'in' THEN 1 ELSE 0 END), 0) as "count!: i64"
               FROM segment_rsvps sr
               JOIN schedule_items si ON si.id = sr.schedule_item_id
               WHERE si.event_id = ?1 AND si.segment_key = ?2"#,
            event_id,
            segment_key,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.count)
    }
}
