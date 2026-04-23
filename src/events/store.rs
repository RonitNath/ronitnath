use sqlx::{SqlitePool, query, query_as};

use crate::events::models::{
    AuditLog, Event, Invitee, InviteeGuest, MessageLog, MessageScript, ScheduleItem,
};

#[derive(Debug, Clone)]
pub(crate) struct EventStore {
    pool: SqlitePool,
}

impl EventStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub(crate) async fn insert_event(&self, event: &Event) -> sqlx::Result<()> {
        query(
            r"
            INSERT INTO events (
                id, slug, title, subtitle, summary, details_markdown,
                approximate_location_name, location_name, address, map_url,
                starts_at, ends_at, timezone,
                status, visibility, signup_mode, self_signup_token_hash,
                self_signup_requires_approval, attendee_cap, display_capacity,
                layout_key, theme_css_path, theme_config_json, notes_label,
                notes_caption, dietary_label, arrival_note_label,
                arrival_note_caption, rsvp_closes_at, allow_rsvp_edits,
                created_by_isoastra_identity_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&event.id)
        .bind(&event.slug)
        .bind(&event.title)
        .bind(&event.subtitle)
        .bind(&event.summary)
        .bind(&event.details_markdown)
        .bind(&event.approximate_location_name)
        .bind(&event.location_name)
        .bind(&event.address)
        .bind(&event.map_url)
        .bind(&event.starts_at)
        .bind(&event.ends_at)
        .bind(&event.timezone)
        .bind(&event.status)
        .bind(&event.visibility)
        .bind(&event.signup_mode)
        .bind(&event.self_signup_token_hash)
        .bind(event.self_signup_requires_approval)
        .bind(event.attendee_cap)
        .bind(event.display_capacity)
        .bind(&event.layout_key)
        .bind(&event.theme_css_path)
        .bind(&event.theme_config_json)
        .bind(&event.notes_label)
        .bind(&event.notes_caption)
        .bind(&event.dietary_label)
        .bind(&event.arrival_note_label)
        .bind(&event.arrival_note_caption)
        .bind(&event.rsvp_closes_at)
        .bind(event.allow_rsvp_edits)
        .bind(&event.created_by_isoastra_identity_id)
        .bind(&event.created_at)
        .bind(&event.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn update_event(&self, event: &Event) -> sqlx::Result<()> {
        query(
            r"
            UPDATE events SET
                slug = ?, title = ?, subtitle = ?, summary = ?,
                details_markdown = ?, approximate_location_name = ?,
                location_name = ?, address = ?, map_url = ?, starts_at = ?,
                ends_at = ?, timezone = ?, status = ?,
                visibility = ?, signup_mode = ?, self_signup_token_hash = ?,
                self_signup_requires_approval = ?, attendee_cap = ?,
                display_capacity = ?, layout_key = ?, theme_css_path = ?,
                theme_config_json = ?, notes_label = ?, notes_caption = ?,
                dietary_label = ?, arrival_note_label = ?,
                arrival_note_caption = ?, rsvp_closes_at = ?,
                allow_rsvp_edits = ?, updated_at = ?
            WHERE id = ?
            ",
        )
        .bind(&event.slug)
        .bind(&event.title)
        .bind(&event.subtitle)
        .bind(&event.summary)
        .bind(&event.details_markdown)
        .bind(&event.approximate_location_name)
        .bind(&event.location_name)
        .bind(&event.address)
        .bind(&event.map_url)
        .bind(&event.starts_at)
        .bind(&event.ends_at)
        .bind(&event.timezone)
        .bind(&event.status)
        .bind(&event.visibility)
        .bind(&event.signup_mode)
        .bind(&event.self_signup_token_hash)
        .bind(event.self_signup_requires_approval)
        .bind(event.attendee_cap)
        .bind(event.display_capacity)
        .bind(&event.layout_key)
        .bind(&event.theme_css_path)
        .bind(&event.theme_config_json)
        .bind(&event.notes_label)
        .bind(&event.notes_caption)
        .bind(&event.dietary_label)
        .bind(&event.arrival_note_label)
        .bind(&event.arrival_note_caption)
        .bind(&event.rsvp_closes_at)
        .bind(event.allow_rsvp_edits)
        .bind(&event.updated_at)
        .bind(&event.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn event_by_ref(&self, event_ref: &str) -> sqlx::Result<Option<Event>> {
        query_as::<_, Event>("SELECT * FROM events WHERE id = ? OR slug = ? LIMIT 1")
            .bind(event_ref)
            .bind(event_ref)
            .fetch_optional(&self.pool)
            .await
    }

    pub(crate) async fn public_events(&self) -> sqlx::Result<Vec<Event>> {
        query_as::<_, Event>(
            "SELECT * FROM events WHERE status = 'published' AND visibility = 'public' ORDER BY starts_at ASC",
        )
        .fetch_all(&self.pool)
        .await
    }

    pub(crate) async fn admin_events(&self) -> sqlx::Result<Vec<Event>> {
        query_as::<_, Event>("SELECT * FROM events ORDER BY starts_at DESC")
            .fetch_all(&self.pool)
            .await
    }

    pub(crate) async fn insert_schedule_item(&self, item: &ScheduleItem) -> sqlx::Result<()> {
        query(
            r"
            INSERT INTO event_schedule_items (
                id, event_id, starts_at, ends_at, title, details,
                location_name, sort_order, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&item.id)
        .bind(&item.event_id)
        .bind(&item.starts_at)
        .bind(&item.ends_at)
        .bind(&item.title)
        .bind(&item.details)
        .bind(&item.location_name)
        .bind(item.sort_order)
        .bind(&item.created_at)
        .bind(&item.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn schedule_for_event(
        &self,
        event_id: &str,
    ) -> sqlx::Result<Vec<ScheduleItem>> {
        query_as::<_, ScheduleItem>(
            "SELECT * FROM event_schedule_items WHERE event_id = ? ORDER BY sort_order ASC, starts_at ASC",
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await
    }

    pub(crate) async fn insert_invitee(&self, invitee: &Invitee) -> sqlx::Result<()> {
        query(
            r"
            INSERT INTO event_invitees (
                id, event_id, display_name, email, phone, invite_token_hash,
                invite_token_version, party_size_limit, rsvp_status,
                arrival_note, dietary_restrictions, general_notes,
                notes_caption_snapshot, personalized_script_key,
                personalized_script_override, sent_at, opened_at,
                responded_at, location_approved, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&invitee.id)
        .bind(&invitee.event_id)
        .bind(&invitee.display_name)
        .bind(&invitee.email)
        .bind(&invitee.phone)
        .bind(&invitee.invite_token_hash)
        .bind(invitee.invite_token_version)
        .bind(invitee.party_size_limit)
        .bind(&invitee.rsvp_status)
        .bind(&invitee.arrival_note)
        .bind(&invitee.dietary_restrictions)
        .bind(&invitee.general_notes)
        .bind(&invitee.notes_caption_snapshot)
        .bind(&invitee.personalized_script_key)
        .bind(&invitee.personalized_script_override)
        .bind(&invitee.sent_at)
        .bind(&invitee.opened_at)
        .bind(&invitee.responded_at)
        .bind(invitee.location_approved)
        .bind(&invitee.created_at)
        .bind(&invitee.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn invitee_by_id(&self, invitee_id: &str) -> sqlx::Result<Option<Invitee>> {
        query_as::<_, Invitee>("SELECT * FROM event_invitees WHERE id = ? LIMIT 1")
            .bind(invitee_id)
            .fetch_optional(&self.pool)
            .await
    }

    pub(crate) async fn invitee_by_token_hash(
        &self,
        token_hash: &str,
    ) -> sqlx::Result<Option<Invitee>> {
        query_as::<_, Invitee>("SELECT * FROM event_invitees WHERE invite_token_hash = ? LIMIT 1")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await
    }

    pub(crate) async fn invitees_for_event(&self, event_id: &str) -> sqlx::Result<Vec<Invitee>> {
        query_as::<_, Invitee>(
            "SELECT * FROM event_invitees WHERE event_id = ? ORDER BY display_name ASC",
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await
    }

    pub(crate) async fn update_invitee_token(
        &self,
        invitee_id: &str,
        token_hash: &str,
        updated_at: &str,
    ) -> sqlx::Result<()> {
        query(
            r"
            UPDATE event_invitees
            SET invite_token_hash = ?,
                invite_token_version = invite_token_version + 1,
                updated_at = ?
            WHERE id = ?
            ",
        )
        .bind(token_hash)
        .bind(updated_at)
        .bind(invitee_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn approve_invitee_location(
        &self,
        invitee_id: &str,
        updated_at: &str,
    ) -> sqlx::Result<()> {
        query(
            r"
            UPDATE event_invitees
            SET location_approved = 1,
                rsvp_status = CASE WHEN location_approved = 0 AND rsvp_status != 'no' THEN 'yes' ELSE rsvp_status END,
                updated_at = ?
            WHERE id = ?
            ",
        )
        .bind(updated_at)
        .bind(invitee_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn update_invitee_opened(
        &self,
        invitee_id: &str,
        opened_at: &str,
    ) -> sqlx::Result<()> {
        query(
            r"
            UPDATE event_invitees
            SET rsvp_status = CASE WHEN rsvp_status = 'invited' THEN 'opened' ELSE rsvp_status END,
                opened_at = COALESCE(opened_at, ?),
                updated_at = ?
            WHERE id = ?
            ",
        )
        .bind(opened_at)
        .bind(opened_at)
        .bind(invitee_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn save_rsvp_with_guests(
        &self,
        invitee: &Invitee,
        guests: &[InviteeGuest],
    ) -> sqlx::Result<()> {
        let mut tx = self.pool.begin().await?;
        query(
            r"
            UPDATE event_invitees
            SET rsvp_status = ?, arrival_note = ?, dietary_restrictions = ?,
                general_notes = ?, responded_at = ?, updated_at = ?
            WHERE id = ?
            ",
        )
        .bind(&invitee.rsvp_status)
        .bind(&invitee.arrival_note)
        .bind(&invitee.dietary_restrictions)
        .bind(&invitee.general_notes)
        .bind(&invitee.responded_at)
        .bind(&invitee.updated_at)
        .bind(&invitee.id)
        .execute(&mut *tx)
        .await?;

        query("DELETE FROM event_invitee_guests WHERE invitee_id = ?")
            .bind(&invitee.id)
            .execute(&mut *tx)
            .await?;

        for guest in guests {
            query(
                r"
                INSERT INTO event_invitee_guests (
                    id, invitee_id, display_name, attending,
                    dietary_restrictions, general_notes, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ",
            )
            .bind(&guest.id)
            .bind(&guest.invitee_id)
            .bind(&guest.display_name)
            .bind(guest.attending)
            .bind(&guest.dietary_restrictions)
            .bind(&guest.general_notes)
            .bind(&guest.created_at)
            .bind(&guest.updated_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn guests_for_invitee(
        &self,
        invitee_id: &str,
    ) -> sqlx::Result<Vec<InviteeGuest>> {
        query_as::<_, InviteeGuest>(
            "SELECT * FROM event_invitee_guests WHERE invitee_id = ? ORDER BY created_at ASC",
        )
        .bind(invitee_id)
        .fetch_all(&self.pool)
        .await
    }

    pub(crate) async fn confirmed_attendee_count(&self, event_id: &str) -> sqlx::Result<i64> {
        let primary: (i64,) = query_as(
            "SELECT COUNT(*) FROM event_invitees WHERE event_id = ? AND rsvp_status = 'yes'",
        )
        .bind(event_id)
        .fetch_one(&self.pool)
        .await?;
        let guests: (i64,) = query_as(
            r"
            SELECT COUNT(*)
            FROM event_invitee_guests g
            JOIN event_invitees i ON i.id = g.invitee_id
            WHERE i.event_id = ? AND i.rsvp_status = 'yes' AND g.attending = 1
            ",
        )
        .bind(event_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(primary.0 + guests.0)
    }

    pub(crate) async fn insert_script(&self, script: &MessageScript) -> sqlx::Result<()> {
        query(
            r"
            INSERT INTO event_message_scripts (
                id, event_id, key, label, body_template,
                sort_order, active, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&script.id)
        .bind(&script.event_id)
        .bind(&script.key)
        .bind(&script.label)
        .bind(&script.body_template)
        .bind(script.sort_order)
        .bind(script.active)
        .bind(&script.created_at)
        .bind(&script.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn script_by_id(
        &self,
        script_id: &str,
    ) -> sqlx::Result<Option<MessageScript>> {
        query_as::<_, MessageScript>("SELECT * FROM event_message_scripts WHERE id = ? LIMIT 1")
            .bind(script_id)
            .fetch_optional(&self.pool)
            .await
    }

    pub(crate) async fn script_by_key(
        &self,
        event_id: &str,
        key: &str,
    ) -> sqlx::Result<Option<MessageScript>> {
        query_as::<_, MessageScript>(
            "SELECT * FROM event_message_scripts WHERE event_id = ? AND key = ? LIMIT 1",
        )
        .bind(event_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
    }

    pub(crate) async fn insert_message_log(&self, log: &MessageLog) -> sqlx::Result<()> {
        query(
            r"
            INSERT INTO event_message_log (
                id, event_id, invitee_id, script_id, actor_isoastra_identity_id,
                kind, recipient, rendered_hash, idempotency_key, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&log.id)
        .bind(&log.event_id)
        .bind(&log.invitee_id)
        .bind(&log.script_id)
        .bind(&log.actor_isoastra_identity_id)
        .bind(&log.kind)
        .bind(&log.recipient)
        .bind(&log.rendered_hash)
        .bind(&log.idempotency_key)
        .bind(&log.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn message_logs_for_invitee(
        &self,
        invitee_id: &str,
    ) -> sqlx::Result<Vec<MessageLog>> {
        query_as::<_, MessageLog>(
            "SELECT * FROM event_message_log WHERE invitee_id = ? ORDER BY created_at DESC",
        )
        .bind(invitee_id)
        .fetch_all(&self.pool)
        .await
    }

    pub(crate) async fn insert_audit_log(&self, log: &AuditLog) -> sqlx::Result<()> {
        query(
            r"
            INSERT INTO event_audit_log (
                id, event_id, actor_isoastra_identity_id,
                action, metadata_json, created_at
            ) VALUES (?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(&log.id)
        .bind(&log.event_id)
        .bind(&log.actor_isoastra_identity_id)
        .bind(&log.action)
        .bind(&log.metadata_json)
        .bind(&log.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn audit_logs_for_event(&self, event_id: &str) -> sqlx::Result<Vec<AuditLog>> {
        query_as::<_, AuditLog>(
            "SELECT * FROM event_audit_log WHERE event_id = ? ORDER BY created_at DESC",
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await
    }
}
