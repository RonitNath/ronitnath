//! Queries and types for the `attendance` table (person × event) — the
//! longitudinal edge — and the guest-facing RSVP upsert.

use serde::Serialize;
use ts_rs::TS;
use utoipa::ToSchema;

use super::Store;

#[derive(Debug, Serialize, sqlx::FromRow, TS, ToSchema)]
#[ts(export)]
pub struct Attendance {
    #[ts(type = "number")]
    pub person_id: i64,
    pub status: String,
    #[ts(type = "number")]
    pub party_size: i64,
    pub note: String,
    pub updated_at: String,
}

/// Attendance joined with the person for the admin guest table.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AttendanceRow {
    pub person_id: i64,
    pub person_name: String,
    pub nickname: String,
    pub group_label: String,
    pub status: String,
    pub party_size: i64,
    pub note: String,
    pub updated_at: String,
    pub segments: String,
}

impl Store {
    pub async fn is_event_attendee(
        &self,
        account_id: i64,
        event_id: i64,
        person_id: i64,
    ) -> sqlx::Result<bool> {
        sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM attendance
                WHERE account_id = ?1 AND event_id = ?2 AND person_id = ?3
                  AND status IN ('going', 'attended')
            ) as "exists!: bool""#,
            account_id,
            event_id,
            person_id,
        )
        .fetch_one(&self.pool)
        .await
    }

    pub async fn find_attendance(
        &self,
        account_id: i64,
        event_id: i64,
        person_id: i64,
    ) -> sqlx::Result<Option<Attendance>> {
        sqlx::query_as!(
            Attendance,
            r#"SELECT person_id as "person_id!: i64", status, party_size as "party_size!: i64",
                      note, updated_at
               FROM attendance
               WHERE account_id = ?1 AND event_id = ?2 AND person_id = ?3"#,
            account_id,
            event_id,
            person_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn upsert_attendance(
        &self,
        account_id: i64,
        event_id: i64,
        person_id: i64,
        status: &str,
        party_size: i64,
        note: &str,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"INSERT INTO attendance (account_id, event_id, person_id, status, party_size, note)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6)
               ON CONFLICT (event_id, person_id) DO UPDATE SET
                   status = excluded.status,
                   party_size = excluded.party_size,
                   note = excluded.note,
                   updated_at = datetime('now')"#,
            account_id,
            event_id,
            person_id,
            status,
            party_size,
            note,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The admin guest table: everyone with an attendance row for the
    /// event, with their per-segment answers folded into a display string
    /// ("dinner:in · sleepover:out").
    pub async fn list_attendance(
        &self,
        account_id: i64,
        event_id: i64,
    ) -> sqlx::Result<Vec<AttendanceRow>> {
        sqlx::query_as!(
            AttendanceRow,
            r#"SELECT a.person_id as "person_id!: i64", p.name as person_name, p.nickname, p.group_label,
                      a.status, a.party_size as "party_size!: i64", a.note, a.updated_at,
                      COALESCE((
                          SELECT GROUP_CONCAT(
                              si.segment_key || ':' || sr.status
                              || CASE WHEN sr.paid = 1 THEN ' paid' ELSE '' END
                              || CASE sr.attended WHEN 1 THEN ' ✓went' WHEN 0 THEN ' ✗skipped' ELSE '' END,
                              ' · ')
                          FROM segment_rsvps sr
                          JOIN schedule_items si
                            ON si.account_id = sr.account_id AND si.id = sr.schedule_item_id
                          WHERE sr.account_id = a.account_id
                            AND sr.person_id = a.person_id
                            AND si.event_id = a.event_id
                      ), '') as "segments!: String"
               FROM attendance a
               JOIN people p ON p.account_id = a.account_id AND p.id = a.person_id
               WHERE a.account_id = ?1 AND a.event_id = ?2
               ORDER BY p.group_label, p.name COLLATE NOCASE"#,
            account_id,
            event_id,
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Headline numbers for the admin page: going / maybe / total heads
    /// (party sizes summed over "going").
    pub async fn attendance_counts(
        &self,
        account_id: i64,
        event_id: i64,
    ) -> sqlx::Result<(i64, i64, i64)> {
        let row = sqlx::query!(
            r#"SELECT
                   COALESCE(SUM(CASE WHEN status = 'going' THEN 1 ELSE 0 END), 0) as "going!: i64",
                   COALESCE(SUM(CASE WHEN status = 'maybe' THEN 1 ELSE 0 END), 0) as "maybe!: i64",
                   COALESCE(SUM(CASE WHEN status = 'going' THEN party_size ELSE 0 END), 0) as "heads!: i64"
               FROM attendance WHERE account_id = ?1 AND event_id = ?2"#,
            account_id,
            event_id,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok((row.going, row.maybe, row.heads))
    }

    /// Live "going" headcount for the public landing page — unscoped like
    /// `Store::list_events_public` (this app is single-instance; there's no
    /// account context to filter by on that surface).
    pub async fn public_attendee_count(&self, event_id: i64) -> sqlx::Result<i64> {
        let row = sqlx::query!(
            r#"SELECT COALESCE(SUM(party_size), 0) as "heads!: i64"
               FROM attendance WHERE event_id = ?1 AND status = 'going'"#,
            event_id,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.heads)
    }
}
