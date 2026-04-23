PRAGMA foreign_keys = ON;

CREATE TABLE events (
    id TEXT PRIMARY KEY,
    slug TEXT UNIQUE,
    title TEXT NOT NULL,
    subtitle TEXT,
    summary TEXT,
    details_markdown TEXT NOT NULL DEFAULT '',
    location_name TEXT,
    address TEXT,
    map_url TEXT,
    starts_at TEXT NOT NULL,
    ends_at TEXT NOT NULL,
    timezone TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'published', 'archived')),
    visibility TEXT NOT NULL DEFAULT 'invite_only'
        CHECK (visibility IN ('public', 'unlisted', 'invite_only')),
    signup_mode TEXT NOT NULL DEFAULT 'invite_only'
        CHECK (signup_mode IN ('invite_only', 'self_signup')),
    self_signup_token_hash TEXT,
    self_signup_requires_approval INTEGER NOT NULL DEFAULT 0
        CHECK (self_signup_requires_approval IN (0, 1)),
    attendee_cap INTEGER CHECK (attendee_cap IS NULL OR attendee_cap > 0),
    display_capacity INTEGER NOT NULL DEFAULT 0
        CHECK (display_capacity IN (0, 1)),
    layout_key TEXT NOT NULL DEFAULT 'default',
    theme_css_path TEXT,
    theme_config_json TEXT NOT NULL DEFAULT '{}',
    notes_label TEXT NOT NULL DEFAULT 'Notes',
    notes_caption TEXT,
    dietary_label TEXT NOT NULL DEFAULT 'Dietary restrictions',
    arrival_note_label TEXT NOT NULL DEFAULT 'Arrival timing',
    arrival_note_caption TEXT,
    rsvp_closes_at TEXT,
    allow_rsvp_edits INTEGER NOT NULL DEFAULT 1
        CHECK (allow_rsvp_edits IN (0, 1)),
    created_by_isoastra_identity_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_events_starts_at ON events(starts_at);
CREATE INDEX idx_events_status ON events(status);
CREATE INDEX idx_events_visibility ON events(visibility);
CREATE INDEX idx_events_self_signup_token_hash ON events(self_signup_token_hash);

CREATE TABLE event_schedule_items (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    starts_at TEXT,
    ends_at TEXT,
    title TEXT NOT NULL,
    details TEXT,
    location_name TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_schedule_event_order ON event_schedule_items(event_id, sort_order);

CREATE TABLE event_invitees (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    email TEXT,
    phone TEXT,
    invite_token_hash TEXT NOT NULL UNIQUE,
    invite_token_version INTEGER NOT NULL DEFAULT 1,
    party_size_limit INTEGER NOT NULL DEFAULT 1 CHECK (party_size_limit >= 1),
    rsvp_status TEXT NOT NULL DEFAULT 'invited'
        CHECK (rsvp_status IN ('invited', 'opened', 'yes', 'no', 'maybe')),
    arrival_note TEXT NOT NULL DEFAULT '',
    dietary_restrictions TEXT NOT NULL DEFAULT '',
    general_notes TEXT NOT NULL DEFAULT '',
    notes_caption_snapshot TEXT,
    personalized_script_key TEXT,
    personalized_script_override TEXT,
    sent_at TEXT,
    opened_at TEXT,
    responded_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_invitees_event ON event_invitees(event_id);
CREATE INDEX idx_invitees_token_hash ON event_invitees(invite_token_hash);
CREATE INDEX idx_invitees_rsvp_status ON event_invitees(rsvp_status);
CREATE INDEX idx_invitees_email ON event_invitees(email);

CREATE TABLE event_invitee_guests (
    id TEXT PRIMARY KEY,
    invitee_id TEXT NOT NULL REFERENCES event_invitees(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    attending INTEGER NOT NULL DEFAULT 1 CHECK (attending IN (0, 1)),
    dietary_restrictions TEXT NOT NULL DEFAULT '',
    general_notes TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_guests_invitee ON event_invitee_guests(invitee_id);

CREATE TABLE event_message_scripts (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    label TEXT NOT NULL,
    body_template TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(event_id, key)
);

CREATE INDEX idx_scripts_event_order ON event_message_scripts(event_id, sort_order);

CREATE TABLE event_invitee_script_overrides (
    id TEXT PRIMARY KEY,
    invitee_id TEXT NOT NULL REFERENCES event_invitees(id) ON DELETE CASCADE,
    script_id TEXT NOT NULL REFERENCES event_message_scripts(id) ON DELETE CASCADE,
    body_template TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(invitee_id, script_id)
);

CREATE TABLE event_message_log (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    invitee_id TEXT REFERENCES event_invitees(id) ON DELETE SET NULL,
    script_id TEXT REFERENCES event_message_scripts(id) ON DELETE SET NULL,
    actor_isoastra_identity_id TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('copied', 'sent')),
    recipient TEXT,
    rendered_hash TEXT,
    idempotency_key TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_message_log_event ON event_message_log(event_id, created_at);
CREATE INDEX idx_message_log_invitee ON event_message_log(invitee_id, created_at);
CREATE UNIQUE INDEX idx_message_log_idempotency
    ON event_message_log(idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE TABLE event_audit_log (
    id TEXT PRIMARY KEY,
    event_id TEXT REFERENCES events(id) ON DELETE SET NULL,
    actor_isoastra_identity_id TEXT,
    action TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX idx_audit_event ON event_audit_log(event_id, created_at);
CREATE INDEX idx_audit_action ON event_audit_log(action, created_at);
