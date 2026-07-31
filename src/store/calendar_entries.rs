//! Standalone calendar entries and their sole redaction chokepoint.

use super::Store;
use crate::access::level::Level;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CalendarEntry {
    pub id: i64,
    pub title: String,
    pub location: String,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub timezone: String,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum CalendarEntryView {
    Busy(CalendarEntryBusyView),
    Entry(CalendarEntryDetailView),
}

#[derive(Debug, Clone, Serialize)]
pub struct CalendarEntryBusyView {
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalendarEntryDetailView {
    pub title: String,
    pub location: Option<String>,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub timezone: String,
    pub notes: Option<String>,
}

impl CalendarEntry {
    pub fn view_for(&self, level: Level) -> Option<CalendarEntryView> {
        match level {
            Level::Hidden => None,
            Level::Busy => Some(CalendarEntryView::Busy(CalendarEntryBusyView {
                starts_at: self.starts_at.clone(),
                ends_at: self.ends_at.clone(),
                timezone: self.timezone.clone(),
            })),
            Level::Summary | Level::Full => {
                let full = level == Level::Full;
                Some(CalendarEntryView::Entry(CalendarEntryDetailView {
                    title: self.title.clone(),
                    location: full.then(|| self.location.clone()),
                    starts_at: self.starts_at.clone(),
                    ends_at: self.ends_at.clone(),
                    timezone: self.timezone.clone(),
                    notes: full.then(|| self.notes.clone()),
                }))
            }
        }
    }
}

pub struct CalendarEntryFields<'a> {
    pub title: &'a str,
    pub location: &'a str,
    pub starts_at: &'a str,
    pub ends_at: Option<&'a str>,
    pub timezone: &'a str,
    pub notes: &'a str,
}

impl Store {
    pub async fn list_calendar_entries(&self, account_id: i64) -> sqlx::Result<Vec<CalendarEntry>> {
        sqlx::query_as!(
            CalendarEntry,
            r#"SELECT id as "id!: i64", title, location, starts_at, ends_at,
            timezone, notes, created_at, updated_at FROM calendar_entries
            WHERE account_id = ?1 ORDER BY starts_at"#,
            account_id
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn list_calendar_entries_range(
        &self,
        account_id: i64,
        start: &str,
        end: &str,
    ) -> sqlx::Result<Vec<CalendarEntry>> {
        sqlx::query_as!(
            CalendarEntry,
            r#"SELECT id as "id!: i64", title, location, starts_at, ends_at,
            timezone, notes, created_at, updated_at FROM calendar_entries
            WHERE account_id = ?1 AND starts_at >= ?2 AND starts_at < ?3 ORDER BY starts_at"#,
            account_id,
            start,
            end
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn find_calendar_entry(
        &self,
        account_id: i64,
        id: i64,
    ) -> sqlx::Result<Option<CalendarEntry>> {
        sqlx::query_as!(
            CalendarEntry,
            r#"SELECT id as "id!: i64", title, location, starts_at, ends_at,
            timezone, notes, created_at, updated_at FROM calendar_entries
            WHERE account_id = ?1 AND id = ?2"#,
            account_id,
            id
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn create_calendar_entry(
        &self,
        account_id: i64,
        fields: &CalendarEntryFields<'_>,
    ) -> sqlx::Result<CalendarEntry> {
        let mut tx = self.pool.begin().await?;
        let entry = sqlx::query_as!(CalendarEntry, r#"INSERT INTO calendar_entries
            (account_id, title, location, starts_at, ends_at, timezone, notes)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            RETURNING id as "id!: i64", title, location, starts_at, ends_at, timezone, notes, created_at, updated_at"#,
            account_id, fields.title, fields.location, fields.starts_at, fields.ends_at,
            fields.timezone, fields.notes).fetch_one(&mut *tx).await?;
        sqlx::query!(
            r#"INSERT INTO audience_policies
            (account_id, subject_type, subject_id, public_level)
            VALUES (?1, 'calendar_entry', ?2, 'hidden')"#,
            account_id,
            entry.id
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(entry)
    }

    pub async fn update_calendar_entry(
        &self,
        account_id: i64,
        id: i64,
        fields: &CalendarEntryFields<'_>,
    ) -> sqlx::Result<u64> {
        Ok(sqlx::query!(
            r#"UPDATE calendar_entries SET title=?3, location=?4, starts_at=?5,
            ends_at=?6, timezone=?7, notes=?8, updated_at=datetime('now')
            WHERE account_id=?1 AND id=?2"#,
            account_id,
            id,
            fields.title,
            fields.location,
            fields.starts_at,
            fields.ends_at,
            fields.timezone,
            fields.notes
        )
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn delete_calendar_entry(&self, account_id: i64, id: i64) -> sqlx::Result<u64> {
        let mut tx = self.pool.begin().await?;
        sqlx::query!("DELETE FROM audience_policies WHERE account_id=?1 AND subject_type='calendar_entry' AND subject_id=?2", account_id, id)
            .execute(&mut *tx).await?;
        let affected = sqlx::query!(
            "DELETE FROM calendar_entries WHERE account_id=?1 AND id=?2",
            account_id,
            id
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn create_atomically_adds_hidden_policy() {
        let store = Store::connect_in_memory().await;
        let (_, account_id) = store
            .signup_with_password("Owner", "owner@example.com", "hash")
            .await
            .unwrap();
        let entry = store
            .create_calendar_entry(
                account_id,
                &CalendarEntryFields {
                    title: "Block",
                    location: "Secret",
                    starts_at: "2026-07-12 10:00",
                    ends_at: None,
                    timezone: "America/Los_Angeles",
                    notes: "Private",
                },
            )
            .await
            .unwrap();
        let policy = store
            .find_audience_policy(account_id, "calendar_entry", entry.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(policy.public_level, "hidden");
    }

    #[test]
    fn view_for_withholds_fields_by_level() {
        let entry = CalendarEntry {
            id: 1,
            title: "Secret title".into(),
            location: "Secret place".into(),
            starts_at: "2026-07-12 10:00".into(),
            ends_at: None,
            timezone: "America/Los_Angeles".into(),
            notes: "Secret notes".into(),
            created_at: "".into(),
            updated_at: "".into(),
        };
        assert!(entry.view_for(Level::Hidden).is_none());
        let busy = serde_json::to_string(&entry.view_for(Level::Busy).unwrap()).unwrap();
        assert!(!busy.contains("Secret title"));
        assert!(!busy.contains("Secret place"));
        assert!(!busy.contains("Secret notes"));
        let summary = serde_json::to_value(entry.view_for(Level::Summary).unwrap()).unwrap();
        assert_eq!(summary["Entry"]["title"], "Secret title");
        assert!(summary["Entry"]["location"].is_null());
        let full = serde_json::to_value(entry.view_for(Level::Full).unwrap()).unwrap();
        assert_eq!(full["Entry"]["location"], "Secret place");
    }
}
