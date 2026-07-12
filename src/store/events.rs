//! Queries and types for the `events` table.
//!
//! `Event` is the full row (admin + private-tier pages). Public-tier
//! rendering must go through [`Event::public_view`] so the private fields
//! (address, entry instructions, private details) can't leak by accident —
//! the tier split is the platform's whole security model.

use serde::Serialize;
use ts_rs::TS;
use utoipa::ToSchema;

use super::Store;

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

/// What a guest sees. `address`/`entry_instructions`/`private_details` are
/// `None` on public-tier links — absent from the JSON, not just empty.
#[derive(Debug, Serialize, TS, ToSchema)]
#[ts(export)]
pub struct EventView {
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
    pub fn public_view(&self, private_tier: bool) -> EventView {
        EventView {
            title: self.title.clone(),
            tagline: self.tagline.clone(),
            starts_at: self.starts_at.clone(),
            ends_at: self.ends_at.clone(),
            timezone: self.timezone.clone(),
            status: self.status.clone(),
            summary: self.summary.clone(),
            area_name: self.area_name.clone(),
            address: private_tier.then(|| self.address.clone()),
            entry_instructions: private_tier.then(|| self.entry_instructions.clone()),
            private_details: private_tier.then(|| self.private_details.clone()),
            notice_html: private_tier.then(|| self.notice_html.clone()),
            quick_plan_html: private_tier.then(|| self.quick_plan_html.clone()),
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

    pub async fn find_event_by_slug(&self, account_id: i64, slug: &str) -> sqlx::Result<Option<Event>> {
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
        sqlx::query_as!(
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
        .fetch_one(&self.pool)
        .await
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
