use chrono::Utc;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::events::capacity;
use crate::events::errors::{EventError, Result};
use crate::events::models::{
    AuditLog, CreateEvent, CreateInvitee, CreateScript, Event, EventStatus, EventVisibility,
    GuestUpdate, Invitee, InviteeGuest, MessageKind, MessageLog, MessageScript, RsvpStatus,
    RsvpUpdate, ScheduleItem, SignupMode, UpdateEvent,
};
use crate::events::scripts::{ScriptContext, rendered_hash};
use crate::events::store::EventStore;
use crate::events::tokens::{TokenHasher, TokenPurpose};
use crate::events::viewer::Viewer;

const ARRIVAL_NOTE_LIMIT: usize = 500;
const DIETARY_LIMIT: usize = 1_000;
const GENERAL_NOTES_LIMIT: usize = 4_000;
const GUEST_NOTES_LIMIT: usize = 1_000;

#[derive(Debug, Clone)]
pub(crate) struct EventService {
    store: EventStore,
    tokens: TokenHasher,
    public_base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CreatedInvitee {
    pub(crate) invitee: Invitee,
    pub(crate) raw_token: String,
    pub(crate) rsvp_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SelfSignupInput {
    pub(crate) display_name: String,
    pub(crate) email: Option<String>,
    pub(crate) phone: Option<String>,
    pub(crate) rsvp: RsvpUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RenderedScript {
    pub(crate) rendered: String,
    pub(crate) rendered_hash: String,
}

impl EventService {
    pub(crate) fn new(store: EventStore, tokens: TokenHasher, public_base_url: String) -> Self {
        Self {
            store,
            tokens,
            public_base_url,
        }
    }

    pub(crate) fn store(&self) -> &EventStore {
        &self.store
    }

    pub(crate) async fn create_event(&self, input: CreateEvent) -> Result<Event> {
        require_nonempty("title", &input.title)?;
        require_nonempty("starts_at", &input.starts_at)?;
        require_nonempty("ends_at", &input.ends_at)?;
        require_nonempty("timezone", &input.timezone)?;
        validate_cap(input.attendee_cap)?;

        let now = now();
        let event = Event {
            id: new_id(),
            slug: input.slug,
            title: input.title,
            subtitle: None,
            summary: None,
            details_markdown: String::new(),
            approximate_location_name: None,
            location_name: None,
            address: None,
            map_url: None,
            starts_at: input.starts_at,
            ends_at: input.ends_at,
            timezone: input.timezone,
            status: EventStatus::Draft.as_str().to_owned(),
            visibility: input.visibility.as_str().to_owned(),
            signup_mode: input.signup_mode.as_str().to_owned(),
            self_signup_token_hash: None,
            self_signup_requires_approval: true,
            attendee_cap: input.attendee_cap,
            display_capacity: false,
            layout_key: "default".to_owned(),
            theme_css_path: None,
            theme_config_json: "{}".to_owned(),
            notes_label: "Notes".to_owned(),
            notes_caption: None,
            dietary_label: "Dietary restrictions".to_owned(),
            arrival_note_label: "Arrival timing".to_owned(),
            arrival_note_caption: None,
            rsvp_closes_at: None,
            allow_rsvp_edits: true,
            created_by_isoastra_identity_id: input.created_by_isoastra_identity_id,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.insert_event(&event).await?;
        self.audit(
            Some(&event.id),
            event.created_by_isoastra_identity_id.as_deref(),
            "event.created",
            serde_json::json!({ "title": event.title }),
        )
        .await?;
        Ok(event)
    }

    pub(crate) async fn update_event(
        &self,
        event_ref: &str,
        input: UpdateEvent,
        actor: Option<&str>,
    ) -> Result<Event> {
        let mut event = self.event_required(event_ref).await?;
        if let Some(slug) = input.slug {
            event.slug = slug;
        }
        if let Some(title) = input.title {
            require_nonempty("title", &title)?;
            event.title = title;
        }
        if let Some(subtitle) = input.subtitle {
            event.subtitle = subtitle;
        }
        if let Some(summary) = input.summary {
            event.summary = summary;
        }
        if let Some(details) = input.details_markdown {
            event.details_markdown = details;
        }
        if let Some(location) = input.approximate_location_name {
            event.approximate_location_name = location;
        }
        if let Some(location) = input.location_name {
            event.location_name = location;
        }
        if let Some(address) = input.address {
            event.address = address;
        }
        if let Some(map_url) = input.map_url {
            event.map_url = map_url;
        }
        if let Some(starts_at) = input.starts_at {
            require_nonempty("starts_at", &starts_at)?;
            event.starts_at = starts_at;
        }
        if let Some(ends_at) = input.ends_at {
            require_nonempty("ends_at", &ends_at)?;
            event.ends_at = ends_at;
        }
        if let Some(timezone) = input.timezone {
            require_nonempty("timezone", &timezone)?;
            event.timezone = timezone;
        }
        if let Some(visibility) = input.visibility {
            visibility.as_str().clone_into(&mut event.visibility);
        }
        if let Some(signup_mode) = input.signup_mode {
            signup_mode.as_str().clone_into(&mut event.signup_mode);
        }
        if let Some(cap) = input.attendee_cap {
            validate_cap(cap)?;
            event.attendee_cap = cap;
        }
        if let Some(display) = input.display_capacity {
            event.display_capacity = display;
        }
        if let Some(requires_approval) = input.self_signup_requires_approval {
            event.self_signup_requires_approval = requires_approval;
        }
        if let Some(layout) = input.layout_key {
            require_nonempty("layout_key", &layout)?;
            event.layout_key = layout;
        }
        if let Some(css) = input.theme_css_path {
            event.theme_css_path = css;
        }
        if let Some(config) = input.theme_config_json {
            event.theme_config_json = config;
        }
        if let Some(label) = input.notes_label {
            require_nonempty("notes_label", &label)?;
            event.notes_label = label;
        }
        if let Some(caption) = input.notes_caption {
            event.notes_caption = caption;
        }
        if let Some(label) = input.dietary_label {
            require_nonempty("dietary_label", &label)?;
            event.dietary_label = label;
        }
        if let Some(label) = input.arrival_note_label {
            require_nonempty("arrival_note_label", &label)?;
            event.arrival_note_label = label;
        }
        if let Some(caption) = input.arrival_note_caption {
            event.arrival_note_caption = caption;
        }
        if let Some(closes_at) = input.rsvp_closes_at {
            event.rsvp_closes_at = closes_at;
        }
        if let Some(allow) = input.allow_rsvp_edits {
            event.allow_rsvp_edits = allow;
        }
        event.updated_at = now();
        self.store.update_event(&event).await?;
        self.audit(
            Some(&event.id),
            actor,
            "event.updated",
            serde_json::json!({ "event_id": event.id }),
        )
        .await?;
        Ok(event)
    }

    pub(crate) async fn set_event_status(
        &self,
        event_ref: &str,
        status: EventStatus,
        actor: Option<&str>,
    ) -> Result<Event> {
        let mut event = self.event_required(event_ref).await?;
        status.as_str().clone_into(&mut event.status);
        event.updated_at = now();
        self.store.update_event(&event).await?;
        self.audit(
            Some(&event.id),
            actor,
            "event.status_changed",
            serde_json::json!({ "status": event.status }),
        )
        .await?;
        Ok(event)
    }

    pub(crate) async fn list_events(&self, viewer: &Viewer) -> Result<Vec<Event>> {
        if viewer.is_admin() {
            Ok(self.store.admin_events().await?)
        } else {
            Ok(self.store.public_events().await?)
        }
    }

    pub(crate) async fn get_event(&self, event_ref: &str, viewer: &Viewer) -> Result<Event> {
        let event = self.event_required(event_ref).await?;
        if viewer.is_admin()
            || (event.status == EventStatus::Published.as_str()
                && event.visibility != EventVisibility::InviteOnly.as_str())
        {
            return Ok(event);
        }
        Err(EventError::NotFound)
    }

    pub(crate) async fn get_signup_event(
        &self,
        event_ref: &str,
        signup_token: Option<&str>,
    ) -> Result<(Event, bool)> {
        let event = self.event_required(event_ref).await?;
        let signed_signup = self.verify_signup_token(&event, signup_token)?;
        if event.status != EventStatus::Published.as_str() {
            return Err(EventError::NotFound);
        }
        if event.visibility != EventVisibility::InviteOnly.as_str() || signed_signup {
            return Ok((event, signed_signup));
        }
        Err(EventError::NotFound)
    }

    pub(crate) async fn create_invitee(&self, input: CreateInvitee) -> Result<CreatedInvitee> {
        self.create_invitee_with_access(input, true, RsvpStatus::Invited)
            .await
    }

    async fn create_invitee_with_access(
        &self,
        input: CreateInvitee,
        location_approved: bool,
        rsvp_status: RsvpStatus,
    ) -> Result<CreatedInvitee> {
        require_nonempty("display_name", &input.display_name)?;
        if input.party_size_limit < 1 {
            return Err(EventError::InvalidInput(
                "party_size_limit must be positive".to_owned(),
            ));
        }
        let event = self.event_required(&input.event_id).await?;
        let (raw_token, token_hash) = self.tokens.generate_hash_pair(TokenPurpose::Invite);
        let now = now();
        let invitee = Invitee {
            id: new_id(),
            event_id: event.id.clone(),
            display_name: input.display_name,
            email: input.email,
            phone: input.phone,
            invite_token_hash: token_hash,
            invite_token_version: 1,
            party_size_limit: input.party_size_limit,
            rsvp_status: rsvp_status.as_str().to_owned(),
            arrival_note: String::new(),
            dietary_restrictions: String::new(),
            general_notes: String::new(),
            notes_caption_snapshot: event.notes_caption.clone(),
            personalized_script_key: None,
            personalized_script_override: None,
            sent_at: None,
            opened_at: None,
            responded_at: None,
            location_approved,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.insert_invitee(&invitee).await?;
        self.audit(
            Some(&event.id),
            None,
            "invitee.created",
            serde_json::json!({ "invitee_id": invitee.id }),
        )
        .await?;
        let rsvp_url = self.rsvp_url(&event, &raw_token);
        Ok(CreatedInvitee {
            invitee,
            raw_token,
            rsvp_url,
        })
    }

    pub(crate) async fn approve_invitee_location(
        &self,
        event_ref: &str,
        invitee_id: &str,
        actor: Option<&str>,
    ) -> Result<Invitee> {
        let event = self.event_required(event_ref).await?;
        let invitee = self
            .store
            .invitee_by_id(invitee_id)
            .await?
            .ok_or(EventError::NotFound)?;
        if invitee.event_id != event.id {
            return Err(EventError::NotFound);
        }
        self.store
            .approve_invitee_location(invitee_id, &now())
            .await?;
        self.audit(
            Some(&event.id),
            actor,
            "invitee.location_approved",
            serde_json::json!({ "invitee_id": invitee_id }),
        )
        .await?;
        self.store
            .invitee_by_id(invitee_id)
            .await?
            .ok_or(EventError::NotFound)
    }

    pub(crate) async fn rotate_invitee_token(&self, invitee_id: &str) -> Result<(String, String)> {
        let invitee = self
            .store
            .invitee_by_id(invitee_id)
            .await?
            .ok_or(EventError::NotFound)?;
        let event = self.event_required(&invitee.event_id).await?;
        let (raw_token, token_hash) = self.tokens.generate_hash_pair(TokenPurpose::Invite);
        self.store
            .update_invitee_token(invitee_id, &token_hash, &now())
            .await?;
        Ok((raw_token.clone(), self.rsvp_url(&event, &raw_token)))
    }

    pub(crate) async fn resolve_invitee_token(
        &self,
        event_ref: &str,
        token: &str,
    ) -> Result<Invitee> {
        let event = self.event_required(event_ref).await?;
        let token_hash = self.tokens.hash(TokenPurpose::Invite, token);
        let invitee = self
            .store
            .invitee_by_token_hash(&token_hash)
            .await?
            .ok_or(EventError::InvalidToken)?;
        if invitee.event_id != event.id
            || !self
                .tokens
                .verify(TokenPurpose::Invite, token, &invitee.invite_token_hash)
        {
            return Err(EventError::InvalidToken);
        }
        self.store
            .update_invitee_opened(&invitee.id, &now())
            .await?;
        self.store
            .invitee_by_id(&invitee.id)
            .await?
            .ok_or(EventError::InvalidToken)
    }

    pub(crate) async fn update_rsvp_by_token(
        &self,
        event_ref: &str,
        token: &str,
        input: RsvpUpdate,
    ) -> Result<Invitee> {
        let mut invitee = self.resolve_invitee_token(event_ref, token).await?;
        let event = self.event_required(&invitee.event_id).await?;
        Self::ensure_rsvp_open(&event)?;
        validate_rsvp(&input)?;
        let attending_guests =
            i64::try_from(input.guests.iter().filter(|guest| guest.attending).count())
                .map_err(|_| EventError::InvalidInput("party size is too large".to_owned()))?;
        let total_party = 1 + attending_guests;
        if total_party > invitee.party_size_limit {
            return Err(EventError::InvalidInput(
                "party size exceeds invitee limit".to_owned(),
            ));
        }
        let now = now();
        if !invitee.location_approved && invitee.rsvp_status != RsvpStatus::No.as_str() {
            invitee.rsvp_status = RsvpStatus::Maybe.as_str().to_owned();
        } else {
            input
                .rsvp_status
                .as_str()
                .clone_into(&mut invitee.rsvp_status);
        }
        invitee.arrival_note = input.arrival_note;
        invitee.dietary_restrictions = input.dietary_restrictions;
        invitee.general_notes = input.general_notes;
        invitee.responded_at = Some(now.clone());
        invitee.updated_at = now.clone();
        let guests: Vec<InviteeGuest> = input
            .guests
            .into_iter()
            .map(|guest| guest_from_update(&invitee.id, guest, &now))
            .collect();
        self.store.save_rsvp_with_guests(&invitee, &guests).await?;
        self.store
            .invitee_by_id(&invitee.id)
            .await?
            .ok_or(EventError::NotFound)
    }

    pub(crate) async fn self_signup(
        &self,
        event_ref: &str,
        signup_token: Option<&str>,
        input: SelfSignupInput,
    ) -> Result<CreatedInvitee> {
        let event = self.event_required(event_ref).await?;
        let signed_signup = self.verify_signup_token(&event, signup_token)?;
        if event.signup_mode != SignupMode::SelfSignup.as_str() && !signed_signup {
            return Err(EventError::SignupClosed);
        }
        capacity::ensure_self_signup_capacity(&self.store, &event).await?;
        let requires_approval = event.self_signup_requires_approval;
        let created = self
            .create_invitee_with_access(
                CreateInvitee {
                    event_id: event.id,
                    display_name: input.display_name,
                    email: input.email,
                    phone: input.phone,
                    party_size_limit: 1 + i64::try_from(input.rsvp.guests.len()).map_err(|_| {
                        EventError::InvalidInput("party size is too large".to_owned())
                    })?,
                },
                !requires_approval,
                if requires_approval {
                    RsvpStatus::Maybe
                } else {
                    RsvpStatus::Invited
                },
            )
            .await?;
        if requires_approval {
            let invitee = self
                .store
                .invitee_by_id(&created.invitee.id)
                .await?
                .ok_or(EventError::NotFound)?;
            self.save_waitlisted_signup(invitee, input.rsvp).await?;
        } else {
            let _ = self
                .update_rsvp_by_token(event_ref, &created.raw_token, input.rsvp)
                .await?;
        }
        Ok(created)
    }

    async fn save_waitlisted_signup(
        &self,
        mut invitee: Invitee,
        input: RsvpUpdate,
    ) -> Result<Invitee> {
        validate_rsvp(&input)?;
        let attending_guests =
            i64::try_from(input.guests.iter().filter(|guest| guest.attending).count())
                .map_err(|_| EventError::InvalidInput("party size is too large".to_owned()))?;
        let total_party = 1 + attending_guests;
        if total_party > invitee.party_size_limit {
            return Err(EventError::InvalidInput(
                "party size exceeds invitee limit".to_owned(),
            ));
        }
        let now = now();
        invitee.rsvp_status = RsvpStatus::Maybe.as_str().to_owned();
        invitee.arrival_note = input.arrival_note;
        invitee.dietary_restrictions = input.dietary_restrictions;
        invitee.general_notes = input.general_notes;
        invitee.responded_at = Some(now.clone());
        invitee.updated_at = now.clone();
        let guests: Vec<InviteeGuest> = input
            .guests
            .into_iter()
            .map(|guest| guest_from_update(&invitee.id, guest, &now))
            .collect();
        self.store.save_rsvp_with_guests(&invitee, &guests).await?;
        self.store
            .invitee_by_id(&invitee.id)
            .await?
            .ok_or(EventError::NotFound)
    }

    pub(crate) async fn create_signup_token(&self, event_ref: &str) -> Result<(String, Event)> {
        let mut event = self.event_required(event_ref).await?;
        let (raw, hash) = self.tokens.generate_hash_pair(TokenPurpose::Signup);
        event.self_signup_token_hash = Some(hash);
        event.updated_at = now();
        self.store.update_event(&event).await?;
        Ok((raw, event))
    }

    pub(crate) async fn create_schedule_item(
        &self,
        event_id: &str,
        title: String,
        sort_order: i64,
    ) -> Result<ScheduleItem> {
        require_nonempty("title", &title)?;
        let event = self.event_required(event_id).await?;
        let now = now();
        let item = ScheduleItem {
            id: new_id(),
            event_id: event.id,
            starts_at: None,
            ends_at: None,
            title,
            details: None,
            location_name: None,
            sort_order,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.insert_schedule_item(&item).await?;
        Ok(item)
    }

    pub(crate) async fn create_script(&self, input: CreateScript) -> Result<MessageScript> {
        require_nonempty("key", &input.key)?;
        require_nonempty("label", &input.label)?;
        require_nonempty("body_template", &input.body_template)?;
        let event = self.event_required(&input.event_id).await?;
        let now = now();
        let script = MessageScript {
            id: new_id(),
            event_id: event.id,
            key: input.key,
            label: input.label,
            body_template: input.body_template,
            sort_order: input.sort_order,
            active: true,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.insert_script(&script).await?;
        Ok(script)
    }

    pub(crate) async fn render_script_for_invitee(
        &self,
        script_id: &str,
        invitee_id: &str,
        raw_token: &str,
    ) -> Result<RenderedScript> {
        let script = self
            .store
            .script_by_id(script_id)
            .await?
            .ok_or(EventError::NotFound)?;
        let invitee = self
            .store
            .invitee_by_id(invitee_id)
            .await?
            .ok_or(EventError::NotFound)?;
        let event = self.event_required(&invitee.event_id).await?;
        if script.event_id != event.id {
            return Err(EventError::InvalidInput(
                "script and invitee event mismatch".to_owned(),
            ));
        }
        let rsvp_url = self.rsvp_url(&event, raw_token);
        let context = ScriptContext::for_invitee(&event, &invitee, &rsvp_url, None);
        let rendered = context.render(&script.body_template)?;
        let hash = rendered_hash(&rendered);
        Ok(RenderedScript {
            rendered,
            rendered_hash: hash,
        })
    }

    pub(crate) async fn log_copy(
        &self,
        event_id: &str,
        invitee_id: Option<&str>,
        script_id: Option<&str>,
        actor: Option<&str>,
        rendered: Option<&str>,
    ) -> Result<MessageLog> {
        let now = now();
        let log = MessageLog {
            id: new_id(),
            event_id: event_id.to_owned(),
            invitee_id: invitee_id.map(ToOwned::to_owned),
            script_id: script_id.map(ToOwned::to_owned),
            actor_isoastra_identity_id: actor.map(ToOwned::to_owned),
            kind: MessageKind::Copied.as_str().to_owned(),
            recipient: None,
            rendered_hash: rendered.map(rendered_hash),
            idempotency_key: None,
            created_at: now,
        };
        self.store.insert_message_log(&log).await?;
        Ok(log)
    }

    async fn event_required(&self, event_ref: &str) -> Result<Event> {
        self.store
            .event_by_ref(event_ref)
            .await?
            .ok_or(EventError::NotFound)
    }

    fn verify_signup_token(&self, event: &Event, signup_token: Option<&str>) -> Result<bool> {
        let Some(hash) = &event.self_signup_token_hash else {
            return Ok(false);
        };
        let Some(token) = signup_token else {
            return Ok(false);
        };
        if self.tokens.verify(TokenPurpose::Signup, token, hash) {
            Ok(true)
        } else {
            Err(EventError::InvalidToken)
        }
    }

    fn ensure_rsvp_open(event: &Event) -> Result<()> {
        if event.allow_rsvp_edits {
            Ok(())
        } else {
            Err(EventError::RsvpClosed)
        }
    }

    fn rsvp_url(&self, event: &Event, raw_token: &str) -> String {
        let event_ref = event.slug.as_deref().unwrap_or(&event.id);
        format!(
            "{}/events/{}/r/{}",
            self.public_base_url.trim_end_matches('/'),
            urlencoding::encode(event_ref),
            urlencoding::encode(raw_token)
        )
    }

    async fn audit(
        &self,
        event_id: Option<&str>,
        actor: Option<&str>,
        action: &str,
        metadata: serde_json::Value,
    ) -> Result<()> {
        let log = AuditLog {
            id: new_id(),
            event_id: event_id.map(ToOwned::to_owned),
            actor_isoastra_identity_id: actor.map(ToOwned::to_owned),
            action: action.to_owned(),
            metadata_json: metadata.to_string(),
            created_at: now(),
        };
        self.store.insert_audit_log(&log).await?;
        Ok(())
    }
}

fn guest_from_update(invitee_id: &str, input: GuestUpdate, now: &str) -> InviteeGuest {
    InviteeGuest {
        id: input.id.unwrap_or_else(new_id),
        invitee_id: invitee_id.to_owned(),
        display_name: input.display_name,
        attending: input.attending,
        dietary_restrictions: input.dietary_restrictions,
        general_notes: input.general_notes,
        created_at: now.to_owned(),
        updated_at: now.to_owned(),
    }
}

fn validate_rsvp(input: &RsvpUpdate) -> Result<()> {
    validate_len("arrival_note", &input.arrival_note, ARRIVAL_NOTE_LIMIT)?;
    validate_len(
        "dietary_restrictions",
        &input.dietary_restrictions,
        DIETARY_LIMIT,
    )?;
    validate_len("general_notes", &input.general_notes, GENERAL_NOTES_LIMIT)?;
    for guest in &input.guests {
        require_nonempty("guest.display_name", &guest.display_name)?;
        validate_len(
            "guest.dietary_restrictions",
            &guest.dietary_restrictions,
            DIETARY_LIMIT,
        )?;
        validate_len(
            "guest.general_notes",
            &guest.general_notes,
            GUEST_NOTES_LIMIT,
        )?;
    }
    Ok(())
}

fn validate_cap(cap: Option<i64>) -> Result<()> {
    if cap.is_some_and(|value| value < 1) {
        return Err(EventError::InvalidInput(
            "attendee_cap must be positive".to_owned(),
        ));
    }
    Ok(())
}

fn require_nonempty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(EventError::InvalidInput(format!("{field} is required")));
    }
    Ok(())
}

fn validate_len(field: &str, value: &str, limit: usize) -> Result<()> {
    if value.chars().count() > limit {
        return Err(EventError::InvalidInput(format!("{field} is too long")));
    }
    Ok(())
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn new_id() -> String {
    Ulid::new().to_string()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{EventService, SelfSignupInput};
    use crate::db;
    use crate::events::models::{
        CreateEvent, CreateInvitee, CreateScript, EventStatus, EventVisibility, GuestUpdate,
        RsvpStatus, RsvpUpdate, SignupMode, UpdateEvent,
    };
    use crate::events::store::EventStore;
    use crate::events::tokens::TokenHasher;
    use crate::events::viewer::Viewer;

    async fn service() -> EventService {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("events.db");
        let url = format!("sqlite://{}", db_path.display());
        let pool = db::open_pool(&url).await.expect("pool");
        db::migrate(&pool).await.expect("migrate");
        std::mem::forget(dir);
        EventService::new(
            EventStore::new(pool),
            TokenHasher::new("test-secret"),
            "https://example.test".to_owned(),
        )
    }

    fn event_input(visibility: EventVisibility, signup_mode: SignupMode) -> CreateEvent {
        CreateEvent {
            slug: None,
            title: "Cooking Social".to_owned(),
            starts_at: "2026-04-25T18:00:00-07:00".to_owned(),
            ends_at: "2026-04-25T22:00:00-07:00".to_owned(),
            timezone: "America/Los_Angeles".to_owned(),
            visibility,
            signup_mode,
            attendee_cap: None,
            created_by_isoastra_identity_id: Some("admin".to_owned()),
        }
    }

    fn rsvp(status: RsvpStatus, guests: Vec<GuestUpdate>) -> RsvpUpdate {
        RsvpUpdate {
            rsvp_status: status,
            arrival_note: "I may be a bit late".to_owned(),
            dietary_restrictions: "No shellfish".to_owned(),
            general_notes: "I'll bring something".to_owned(),
            guests,
        }
    }

    fn guest(name: &str) -> GuestUpdate {
        GuestUpdate {
            id: None,
            display_name: name.to_owned(),
            attending: true,
            dietary_restrictions: String::new(),
            general_notes: String::new(),
        }
    }

    #[tokio::test]
    async fn migration_event_visibility_and_audit_work() {
        let svc = service().await;
        let event = svc
            .create_event(event_input(EventVisibility::Public, SignupMode::InviteOnly))
            .await
            .expect("create event");

        let public_events = svc
            .list_events(&Viewer::Anonymous)
            .await
            .expect("public events");
        assert!(public_events.is_empty());

        let published = svc
            .set_event_status(&event.id, EventStatus::Published, Some("admin"))
            .await
            .expect("publish");
        assert_eq!(published.status, "published");

        let public_events = svc
            .list_events(&Viewer::Anonymous)
            .await
            .expect("public events");
        assert_eq!(public_events.len(), 1);

        let audit = svc
            .store()
            .audit_logs_for_event(&event.id)
            .await
            .expect("audit");
        assert!(audit.iter().any(|row| row.action == "event.created"));
        assert!(audit.iter().any(|row| row.action == "event.status_changed"));
    }

    #[tokio::test]
    async fn invite_token_rsvp_and_first_class_guests_work() {
        let svc = service().await;
        let event = svc
            .create_event(event_input(
                EventVisibility::InviteOnly,
                SignupMode::InviteOnly,
            ))
            .await
            .expect("create event");
        let created = svc
            .create_invitee(CreateInvitee {
                event_id: event.id.clone(),
                display_name: "Ada".to_owned(),
                email: Some("ada@example.test".to_owned()),
                phone: None,
                party_size_limit: 2,
            })
            .await
            .expect("create invitee");

        assert_ne!(created.raw_token, created.invitee.invite_token_hash);
        assert!(created.rsvp_url.contains("/events/"));

        let updated = svc
            .update_rsvp_by_token(
                &event.id,
                &created.raw_token,
                rsvp(RsvpStatus::Yes, vec![guest("Grace")]),
            )
            .await
            .expect("update rsvp");
        assert_eq!(updated.rsvp_status, "yes");

        let guests = svc
            .store()
            .guests_for_invitee(&updated.id)
            .await
            .expect("guests");
        assert_eq!(guests.len(), 1);
        assert_eq!(guests[0].display_name, "Grace");

        let too_many = svc
            .update_rsvp_by_token(
                &event.id,
                &created.raw_token,
                rsvp(RsvpStatus::Yes, vec![guest("One"), guest("Two")]),
            )
            .await;
        assert!(too_many.is_err());
    }

    #[tokio::test]
    async fn self_signup_respects_capacity_but_admin_invites_do_not() {
        let svc = service().await;
        let mut input = event_input(EventVisibility::Public, SignupMode::SelfSignup);
        input.attendee_cap = Some(1);
        let event = svc.create_event(input).await.expect("create event");
        let event = svc
            .update_event(
                &event.id,
                UpdateEvent {
                    self_signup_requires_approval: Some(false),
                    ..UpdateEvent::default()
                },
                Some("admin"),
            )
            .await
            .expect("disable approval");

        let first = svc
            .self_signup(
                &event.id,
                None,
                SelfSignupInput {
                    display_name: "First".to_owned(),
                    email: None,
                    phone: None,
                    rsvp: rsvp(RsvpStatus::Yes, Vec::new()),
                },
            )
            .await
            .expect("first signup");
        assert_eq!(first.invitee.display_name, "First");

        let second = svc
            .self_signup(
                &event.id,
                None,
                SelfSignupInput {
                    display_name: "Second".to_owned(),
                    email: None,
                    phone: None,
                    rsvp: rsvp(RsvpStatus::Yes, Vec::new()),
                },
            )
            .await;
        assert!(second.is_err());

        let admin_added = svc
            .create_invitee(CreateInvitee {
                event_id: event.id.clone(),
                display_name: "Admin Added".to_owned(),
                email: None,
                phone: None,
                party_size_limit: 1,
            })
            .await
            .expect("admin add");
        let _ = svc
            .update_rsvp_by_token(
                &event.id,
                &admin_added.raw_token,
                rsvp(RsvpStatus::Yes, Vec::new()),
            )
            .await
            .expect("admin rsvp");

        let display = crate::events::capacity::display(svc.store(), &event)
            .await
            .expect("capacity");
        assert_eq!(display.confirmed, 2);
        assert_eq!(display.public_confirmed, 1);
        assert_eq!(display.over_cap, 1);
    }

    #[tokio::test]
    async fn signed_signup_token_allows_private_signup() {
        let svc = service().await;
        let event = svc
            .create_event(event_input(
                EventVisibility::InviteOnly,
                SignupMode::InviteOnly,
            ))
            .await
            .expect("create event");
        let published = svc
            .set_event_status(&event.id, EventStatus::Published, Some("admin"))
            .await
            .expect("publish");
        let (token, _) = svc
            .create_signup_token(&published.id)
            .await
            .expect("signup token");

        let unsigned = svc.get_signup_event(&published.id, None).await;
        assert!(unsigned.is_err());

        let (signed_event, signed) = svc
            .get_signup_event(&published.id, Some(&token))
            .await
            .expect("signed signup event");
        assert_eq!(signed_event.id, published.id);
        assert!(signed);

        let created = svc
            .self_signup(
                &published.id,
                Some(&token),
                SelfSignupInput {
                    display_name: "Token Guest".to_owned(),
                    email: None,
                    phone: None,
                    rsvp: rsvp(RsvpStatus::Yes, Vec::new()),
                },
            )
            .await
            .expect("signed private signup");
        assert_eq!(created.invitee.display_name, "Token Guest");
        assert_eq!(created.invitee.rsvp_status, "maybe");
        assert!(!created.invitee.location_approved);
        let approved = svc
            .approve_invitee_location(&published.id, &created.invitee.id, Some("admin"))
            .await
            .expect("approve");
        assert_eq!(approved.rsvp_status, "yes");
        assert!(approved.location_approved);
    }

    #[tokio::test]
    async fn script_rendering_and_copy_log_work() {
        let svc = service().await;
        let event = svc
            .create_event(event_input(EventVisibility::Public, SignupMode::InviteOnly))
            .await
            .expect("create event");
        let invitee = svc
            .create_invitee(CreateInvitee {
                event_id: event.id.clone(),
                display_name: "Lin".to_owned(),
                email: None,
                phone: None,
                party_size_limit: 1,
            })
            .await
            .expect("invitee");
        let script = svc
            .create_script(CreateScript {
                event_id: event.id.clone(),
                key: "default".to_owned(),
                label: "Default".to_owned(),
                body_template: "Hi {{ invitee.name }}, RSVP here: {{ rsvp_url }}".to_owned(),
                sort_order: 0,
            })
            .await
            .expect("script");

        let rendered = svc
            .render_script_for_invitee(&script.id, &invitee.invitee.id, &invitee.raw_token)
            .await
            .expect("render");
        assert!(rendered.rendered.contains("Hi Lin"));
        assert!(rendered.rendered.contains(&invitee.raw_token));

        let log = svc
            .log_copy(
                &event.id,
                Some(&invitee.invitee.id),
                Some(&script.id),
                Some("admin"),
                Some(&rendered.rendered),
            )
            .await
            .expect("copy log");
        assert_eq!(log.kind, "copied");

        let logs = svc
            .store()
            .message_logs_for_invitee(&invitee.invitee.id)
            .await
            .expect("logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].rendered_hash, Some(rendered.rendered_hash));
    }
}
