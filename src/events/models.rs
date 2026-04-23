use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventStatus {
    Draft,
    Published,
    Archived,
}

impl EventStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventVisibility {
    Public,
    Unlisted,
    InviteOnly,
}

impl EventVisibility {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Unlisted => "unlisted",
            Self::InviteOnly => "invite_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SignupMode {
    InviteOnly,
    SelfSignup,
}

impl SignupMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::InviteOnly => "invite_only",
            Self::SelfSignup => "self_signup",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RsvpStatus {
    Invited,
    Opened,
    Yes,
    No,
    Maybe,
}

impl RsvpStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Invited => "invited",
            Self::Opened => "opened",
            Self::Yes => "yes",
            Self::No => "no",
            Self::Maybe => "maybe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MessageKind {
    Copied,
    Sent,
}

impl MessageKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Copied => "copied",
            Self::Sent => "sent",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub(crate) struct Event {
    pub(crate) id: String,
    pub(crate) slug: Option<String>,
    pub(crate) title: String,
    pub(crate) subtitle: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) details_markdown: String,
    pub(crate) approximate_location_name: Option<String>,
    pub(crate) location_name: Option<String>,
    pub(crate) address: Option<String>,
    pub(crate) map_url: Option<String>,
    pub(crate) starts_at: String,
    pub(crate) ends_at: String,
    pub(crate) timezone: String,
    pub(crate) status: String,
    pub(crate) visibility: String,
    pub(crate) signup_mode: String,
    pub(crate) self_signup_token_hash: Option<String>,
    pub(crate) self_signup_requires_approval: bool,
    pub(crate) attendee_cap: Option<i64>,
    pub(crate) display_capacity: bool,
    pub(crate) layout_key: String,
    pub(crate) theme_css_path: Option<String>,
    pub(crate) theme_config_json: String,
    pub(crate) notes_label: String,
    pub(crate) notes_caption: Option<String>,
    pub(crate) dietary_label: String,
    pub(crate) arrival_note_label: String,
    pub(crate) arrival_note_caption: Option<String>,
    pub(crate) rsvp_closes_at: Option<String>,
    pub(crate) allow_rsvp_edits: bool,
    pub(crate) created_by_isoastra_identity_id: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub(crate) struct ScheduleItem {
    pub(crate) id: String,
    pub(crate) event_id: String,
    pub(crate) starts_at: Option<String>,
    pub(crate) ends_at: Option<String>,
    pub(crate) title: String,
    pub(crate) details: Option<String>,
    pub(crate) location_name: Option<String>,
    pub(crate) sort_order: i64,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub(crate) struct Invitee {
    pub(crate) id: String,
    pub(crate) event_id: String,
    pub(crate) display_name: String,
    pub(crate) email: Option<String>,
    pub(crate) phone: Option<String>,
    pub(crate) invite_token_hash: String,
    pub(crate) invite_token_version: i64,
    pub(crate) party_size_limit: i64,
    pub(crate) rsvp_status: String,
    pub(crate) arrival_note: String,
    pub(crate) dietary_restrictions: String,
    pub(crate) general_notes: String,
    pub(crate) notes_caption_snapshot: Option<String>,
    pub(crate) personalized_script_key: Option<String>,
    pub(crate) personalized_script_override: Option<String>,
    pub(crate) sent_at: Option<String>,
    pub(crate) opened_at: Option<String>,
    pub(crate) responded_at: Option<String>,
    pub(crate) location_approved: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub(crate) struct InviteeGuest {
    pub(crate) id: String,
    pub(crate) invitee_id: String,
    pub(crate) display_name: String,
    pub(crate) attending: bool,
    pub(crate) dietary_restrictions: String,
    pub(crate) general_notes: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub(crate) struct MessageScript {
    pub(crate) id: String,
    pub(crate) event_id: String,
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) body_template: String,
    pub(crate) sort_order: i64,
    pub(crate) active: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub(crate) struct InviteeScriptOverride {
    pub(crate) id: String,
    pub(crate) invitee_id: String,
    pub(crate) script_id: String,
    pub(crate) body_template: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub(crate) struct MessageLog {
    pub(crate) id: String,
    pub(crate) event_id: String,
    pub(crate) invitee_id: Option<String>,
    pub(crate) script_id: Option<String>,
    pub(crate) actor_isoastra_identity_id: Option<String>,
    pub(crate) kind: String,
    pub(crate) recipient: Option<String>,
    pub(crate) rendered_hash: Option<String>,
    pub(crate) idempotency_key: Option<String>,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub(crate) struct AuditLog {
    pub(crate) id: String,
    pub(crate) event_id: Option<String>,
    pub(crate) actor_isoastra_identity_id: Option<String>,
    pub(crate) action: String,
    pub(crate) metadata_json: String,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CreateEvent {
    pub(crate) slug: Option<String>,
    pub(crate) title: String,
    pub(crate) starts_at: String,
    pub(crate) ends_at: String,
    pub(crate) timezone: String,
    pub(crate) visibility: EventVisibility,
    pub(crate) signup_mode: SignupMode,
    pub(crate) attendee_cap: Option<i64>,
    pub(crate) created_by_isoastra_identity_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(clippy::option_option)]
pub(crate) struct UpdateEvent {
    pub(crate) slug: Option<Option<String>>,
    pub(crate) title: Option<String>,
    pub(crate) subtitle: Option<Option<String>>,
    pub(crate) summary: Option<Option<String>>,
    pub(crate) details_markdown: Option<String>,
    pub(crate) approximate_location_name: Option<Option<String>>,
    pub(crate) location_name: Option<Option<String>>,
    pub(crate) address: Option<Option<String>>,
    pub(crate) map_url: Option<Option<String>>,
    pub(crate) starts_at: Option<String>,
    pub(crate) ends_at: Option<String>,
    pub(crate) timezone: Option<String>,
    pub(crate) visibility: Option<EventVisibility>,
    pub(crate) signup_mode: Option<SignupMode>,
    pub(crate) attendee_cap: Option<Option<i64>>,
    pub(crate) display_capacity: Option<bool>,
    pub(crate) self_signup_requires_approval: Option<bool>,
    pub(crate) layout_key: Option<String>,
    pub(crate) theme_css_path: Option<Option<String>>,
    pub(crate) theme_config_json: Option<String>,
    pub(crate) notes_label: Option<String>,
    pub(crate) notes_caption: Option<Option<String>>,
    pub(crate) dietary_label: Option<String>,
    pub(crate) arrival_note_label: Option<String>,
    pub(crate) arrival_note_caption: Option<Option<String>>,
    pub(crate) rsvp_closes_at: Option<Option<String>>,
    pub(crate) allow_rsvp_edits: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CreateInvitee {
    pub(crate) event_id: String,
    pub(crate) display_name: String,
    pub(crate) email: Option<String>,
    pub(crate) phone: Option<String>,
    pub(crate) party_size_limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RsvpUpdate {
    pub(crate) rsvp_status: RsvpStatus,
    pub(crate) arrival_note: String,
    pub(crate) dietary_restrictions: String,
    pub(crate) general_notes: String,
    pub(crate) guests: Vec<GuestUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GuestUpdate {
    pub(crate) id: Option<String>,
    pub(crate) display_name: String,
    pub(crate) attending: bool,
    pub(crate) dietary_restrictions: String,
    pub(crate) general_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CreateScript {
    pub(crate) event_id: String,
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) body_template: String,
    pub(crate) sort_order: i64,
}
