//! Queries and types for the `events` table.
//!
//! `Event` is the full row. Guest rendering must go through
//! [`Event::view_for`], the subject's sole redaction chokepoint.

use serde::Serialize;
use ts_rs::TS;
use utoipa::ToSchema;

use super::Store;
use crate::access::level::Level;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Event {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub tagline: String,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub timezone: String,
    pub status: String,
    pub summary: String,
    pub area_name: String,
    pub address: String,
    pub entry_instructions: String,
    pub private_details: String,
    /// Display-only attendee number for the landing page (social proof) —
    /// set by hand, never derived from attendance. `None` hides it.
    pub headcount: Option<i64>,
    /// Invite-page notice banner (admin-authored HTML). Private tier only —
    /// it carries contact info.
    pub notice_html: String,
    /// The highly-abridged "day at a glance" plan (admin-authored HTML).
    /// Private tier only.
    pub quick_plan_html: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A Busy view is deliberately opaque: even the event title is withheld.
#[derive(Debug, Serialize, TS, ToSchema)]
#[ts(export)]
#[serde(untagged)]
pub enum EventView {
    Busy(BusyView),
    Event(Box<EventDetailView>),
}

#[derive(Debug, Serialize, TS, ToSchema)]
#[ts(export)]
pub struct BusyView {
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub timezone: String,
}

/// Summary/Full event fields. Private fields are absent at Summary.
#[derive(Debug, Serialize, TS, ToSchema)]
#[ts(export)]
pub struct EventDetailView {
    pub title: String,
    pub tagline: String,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub timezone: String,
    pub status: String,
    pub summary: String,
    pub area_name: String,
    pub address: Option<String>,
    pub entry_instructions: Option<String>,
    pub private_details: Option<String>,
    pub notice_html: Option<String>,
    pub quick_plan_html: Option<String>,
}

impl Event {
    pub fn view_for(&self, level: Level) -> Option<EventView> {
        match level {
            Level::Hidden => None,
            Level::Busy => Some(EventView::Busy(BusyView {
                starts_at: self.starts_at.clone(),
                ends_at: self.ends_at.clone(),
                timezone: self.timezone.clone(),
            })),
            Level::Summary | Level::Full => {
                let full = level == Level::Full;
                Some(EventView::Event(Box::new(EventDetailView {
                    title: self.title.clone(),
                    tagline: self.tagline.clone(),
                    starts_at: self.starts_at.clone(),
                    ends_at: self.ends_at.clone(),
                    timezone: self.timezone.clone(),
                    status: self.status.clone(),
                    summary: self.summary.clone(),
                    area_name: self.area_name.clone(),
                    address: full.then(|| self.address.clone()),
                    entry_instructions: full.then(|| self.entry_instructions.clone()),
                    private_details: full.then(|| self.private_details.clone()),
                    notice_html: full.then(|| self.notice_html.clone()),
                    quick_plan_html: full.then(|| self.quick_plan_html.clone()),
                })))
            }
        }
    }
}

/// Which invite-content column [`Store::set_invite_content`] targets.
#[derive(Debug, Clone, Copy)]
pub enum InviteField {
    Notice,
    QuickPlan,
}

/// The fields an admin edits in one form; everything else is derived.
pub struct EventFields {
    pub slug: String,
    pub title: String,
    pub tagline: String,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub timezone: String,
    pub status: String,
    pub summary: String,
    pub area_name: String,
    pub address: String,
    pub entry_instructions: String,
    pub private_details: String,
    pub notice_html: String,
    pub quick_plan_html: String,
}

impl Store {
    pub async fn list_events(&self, account_id: i64) -> sqlx::Result<Vec<Event>> {
        sqlx::query_as!(
            Event,
            r#"SELECT id as "id!: i64", slug, title, tagline, starts_at, ends_at, timezone,
                      status, summary, area_name, address, entry_instructions, private_details,
                      headcount, notice_html, quick_plan_html, created_at, updated_at
               FROM events WHERE account_id = ?1 ORDER BY starts_at DESC"#,
            account_id,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn find_event(&self, account_id: i64, event_id: i64) -> sqlx::Result<Option<Event>> {
        sqlx::query_as!(
            Event,
            r#"SELECT id as "id!: i64", slug, title, tagline, starts_at, ends_at, timezone,
                      status, summary, area_name, address, entry_instructions, private_details,
                      headcount, notice_html, quick_plan_html, created_at, updated_at
               FROM events WHERE account_id = ?1 AND id = ?2"#,
            account_id,
            event_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn find_event_by_slug(
        &self,
        account_id: i64,
        slug: &str,
    ) -> sqlx::Result<Option<Event>> {
        sqlx::query_as!(
            Event,
            r#"SELECT id as "id!: i64", slug, title, tagline, starts_at, ends_at, timezone,
                      status, summary, area_name, address, entry_instructions, private_details,
                      headcount, notice_html, quick_plan_html, created_at, updated_at
               FROM events WHERE account_id = ?1 AND slug = ?2"#,
            account_id,
            slug,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn create_event(
        &self,
        account_id: i64,
        slug: &str,
        title: &str,
        starts_at: &str,
    ) -> sqlx::Result<Event> {
        let mut tx = self.pool.begin().await?;
        let event = sqlx::query_as!(
            Event,
            r#"INSERT INTO events (account_id, slug, title, starts_at)
               VALUES (?1, ?2, ?3, ?4)
               RETURNING id as "id!: i64", slug, title, tagline, starts_at, ends_at, timezone,
                         status, summary, area_name, address, entry_instructions, private_details,
                         headcount, notice_html, quick_plan_html, created_at, updated_at"#,
            account_id,
            slug,
            title,
            starts_at,
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query!(
            r#"INSERT INTO audience_policies
            (account_id, subject_type, subject_id, public_level)
            VALUES (?1, 'event', ?2, 'hidden')"#,
            account_id,
            event.id
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(event)
    }

    pub async fn update_event(
        &self,
        account_id: i64,
        event_id: i64,
        fields: &EventFields,
    ) -> sqlx::Result<u64> {
        let result = sqlx::query!(
            r#"UPDATE events
               SET slug = ?3, title = ?4, tagline = ?5, starts_at = ?6, ends_at = ?7,
                   timezone = ?8, status = ?9, summary = ?10, area_name = ?11,
                   address = ?12, entry_instructions = ?13, private_details = ?14,
                   notice_html = ?15, quick_plan_html = ?16,
                   updated_at = datetime('now')
               WHERE account_id = ?1 AND id = ?2"#,
            account_id,
            event_id,
            fields.slug,
            fields.title,
            fields.tagline,
            fields.starts_at,
            fields.ends_at,
            fields.timezone,
            fields.status,
            fields.summary,
            fields.area_name,
            fields.address,
            fields.entry_instructions,
            fields.private_details,
            fields.notice_html,
            fields.quick_plan_html,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Updates one invite-content field on its own — what the `set-invite`
    /// CLI uses so agents can adjust the banner/quick-plan without
    /// re-sending the whole event form.
    pub async fn set_invite_content(
        &self,
        account_id: i64,
        event_id: i64,
        field: InviteField,
        html: &str,
    ) -> sqlx::Result<u64> {
        let result = match field {
            InviteField::Notice => {
                sqlx::query!(
                    r#"UPDATE events SET notice_html = ?3, updated_at = datetime('now')
                       WHERE account_id = ?1 AND id = ?2"#,
                    account_id,
                    event_id,
                    html,
                )
                .execute(&self.pool)
                .await?
            }
            InviteField::QuickPlan => {
                sqlx::query!(
                    r#"UPDATE events SET quick_plan_html = ?3, updated_at = datetime('now')
                       WHERE account_id = ?1 AND id = ?2"#,
                    account_id,
                    event_id,
                    html,
                )
                .execute(&self.pool)
                .await?
            }
        };
        Ok(result.rows_affected())
    }

    /// Sets the display-only headcount shown on the landing page.
    pub async fn set_event_headcount(
        &self,
        account_id: i64,
        event_id: i64,
        headcount: Option<i64>,
    ) -> sqlx::Result<u64> {
        let result = sqlx::query!(
            r#"UPDATE events SET headcount = ?3, updated_at = datetime('now')
               WHERE account_id = ?1 AND id = ?2"#,
            account_id,
            event_id,
            headcount,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Every non-draft event on the instance, newest first — the landing
    /// page's data. Deliberately unscoped: the landing page has no account
    /// context (gather is a single-host instance), and only public-safe
    /// fields may be rendered from it (title/tagline/dates/area/summary/
    /// headcount — never address or entry details).
    pub async fn list_events_public(&self) -> sqlx::Result<Vec<Event>> {
        sqlx::query_as!(
            Event,
            r#"SELECT id as "id!: i64", slug, title, tagline, starts_at, ends_at, timezone,
                      status, summary, area_name, address, entry_instructions, private_details,
                      headcount, notice_html, quick_plan_html, created_at, updated_at
               FROM events WHERE status != 'draft'
               ORDER BY starts_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_event_atomically_creates_hidden_audience_policy() {
        let store = Store::connect_in_memory().await;
        let (_, account_id) = store
            .signup_with_password("Owner", "owner@example.com", "hash")
            .await
            .unwrap();
        let event = store
            .create_event(account_id, "invariant", "Invariant", "2026-07-04 12:00")
            .await
            .unwrap();
        let policy = store
            .find_audience_policy(account_id, "event", event.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(policy.public_level, "hidden");
    }

    #[test]
    fn view_for_is_the_complete_event_redaction_chokepoint() {
        let event = Event {
            id: 1,
            slug: "private-party".into(),
            title: "Private Party".into(),
            tagline: "Secret tagline".into(),
            starts_at: "2026-07-04 13:00".into(),
            ends_at: Some("2026-07-04 15:00".into()),
            timezone: "America/Los_Angeles".into(),
            status: "published".into(),
            summary: "Summary".into(),
            area_name: "San Francisco".into(),
            address: "1 Secret St".into(),
            entry_instructions: "Secret entry".into(),
            private_details: "Private details".into(),
            notice_html: "Private notice".into(),
            quick_plan_html: "Private plan".into(),
            headcount: None,
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
        };

        assert!(event.view_for(Level::Hidden).is_none());

        let busy = serde_json::to_value(event.view_for(Level::Busy).unwrap()).unwrap();
        assert_eq!(busy["starts_at"], "2026-07-04 13:00");
        assert!(busy.get("title").is_none());
        assert!(busy.get("address").is_none());
        assert!(busy.get("entry_instructions").is_none());

        let summary = serde_json::to_value(event.view_for(Level::Summary).unwrap()).unwrap();
        assert_eq!(summary["title"], "Private Party");
        assert!(summary["address"].is_null());
        assert!(summary["entry_instructions"].is_null());

        let full = serde_json::to_value(event.view_for(Level::Full).unwrap()).unwrap();
        assert_eq!(full["address"], "1 Secret St");
        assert_eq!(full["entry_instructions"], "Secret entry");
    }
}
