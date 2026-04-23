use askama::Template;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc};
use serde::Serialize;

use crate::AppState;
use crate::events::capacity::CapacityDisplay;
use crate::events::models::{Event, Invitee, InviteeGuest, ScheduleItem};
use crate::events::viewer::Viewer;

pub(crate) fn render_home(state: &AppState) -> Response {
    let template = HomeTemplate {
        script_src: state.manifest.entry("site.ts"),
        css_files: state.manifest.css_for_entry("site.ts"),
    };
    html(template.render())
}

pub(crate) fn render_events_list(state: &AppState, viewer: &Viewer, events: &[Event]) -> Response {
    let is_admin = viewer.is_admin();
    let rows: Vec<EventsListRow> = events
        .iter()
        .map(|e| EventsListRow {
            ref_: ref_for(e),
            title: e.title.clone(),
            subtitle: e.subtitle.clone(),
            summary: e.summary.clone(),
            starts_at: format_datetime_display(&e.starts_at),
            location_name: e.approximate_location_name.clone(),
            status: e.status.clone(),
            visibility: e.visibility.clone(),
            signup_mode: e.signup_mode.clone(),
        })
        .collect();
    let template = EventsListTemplate {
        script_src: state.manifest.entry("site.ts"),
        css_files: state.manifest.css_for_entry("site.ts"),
        is_admin,
        events: rows,
        admin_bootstrap: if is_admin {
            Some(json_attr(&AdminListBootstrap {}))
        } else {
            None
        },
    };
    html(template.render())
}

#[derive(Debug)]
pub(crate) struct AdminExtras {
    pub(crate) confirmed: i64,
    pub(crate) over_cap: i64,
    pub(crate) invitees: Vec<Invitee>,
}

pub(crate) fn render_event_detail(
    state: &AppState,
    viewer: &Viewer,
    event: &Event,
    schedule: &[ScheduleItem],
    cap: &CapacityDisplay,
    admin_extras: Option<&AdminExtras>,
) -> Response {
    let is_admin = viewer.is_admin();
    let event_ref = ref_for(event);
    let location = location_for_event(event, is_admin);
    let signup_open_public = cap.self_signup_open && event.signup_mode == "self_signup";
    let admin_bootstrap = admin_extras.map(|extras| {
        json_attr(&AdminEventBootstrap {
            event_ref: event_ref.clone(),
            event_id: event.id.clone(),
            confirmed: extras.confirmed,
            over_cap: extras.over_cap,
            public_confirmed: cap.public_confirmed,
            cap: cap.cap,
            title: event.title.clone(),
            subtitle: event.subtitle.clone(),
            summary: event.summary.clone(),
            details_markdown: event.details_markdown.clone(),
            approximate_location_name: event.approximate_location_name.clone(),
            location_name: event.location_name.clone(),
            address: event.address.clone(),
            map_url: event.map_url.clone(),
            display_capacity: event.display_capacity,
            self_signup_requires_approval: event.self_signup_requires_approval,
            notes_label: event.notes_label.clone(),
            notes_caption: event.notes_caption.clone(),
            dietary_label: event.dietary_label.clone(),
            arrival_note_label: event.arrival_note_label.clone(),
            arrival_note_caption: event.arrival_note_caption.clone(),
            allow_rsvp_edits: event.allow_rsvp_edits,
            status: event.status.clone(),
            visibility: event.visibility.clone(),
            signup_mode: event.signup_mode.clone(),
            starts_at: event.starts_at.clone(),
            ends_at: event.ends_at.clone(),
            timezone: event.timezone.clone(),
            attendee_cap: event.attendee_cap,
            self_signup_token_set: event.self_signup_token_hash.is_some(),
            invitees: extras
                .invitees
                .iter()
                .map(|i| AdminInviteeRow {
                    id: i.id.clone(),
                    display_name: i.display_name.clone(),
                    email: i.email.clone(),
                    rsvp_status: i.rsvp_status.clone(),
                    party_size_limit: i.party_size_limit,
                    location_approved: i.location_approved,
                    opened_at: i.opened_at.clone(),
                    responded_at: i.responded_at.clone(),
                })
                .collect(),
        })
    });
    let template = EventDetailTemplate {
        script_src: state.manifest.entry("site.ts"),
        css_files: state.manifest.css_for_entry("site.ts"),
        is_admin,
        event_ref: event_ref.clone(),
        title: event.title.clone(),
        subtitle: event.subtitle.clone(),
        summary: event.summary.clone(),
        details_markdown: event.details_markdown.clone(),
        location_name: location.name,
        address: location.address,
        map_url: location.map_url,
        when: format_event_window(&event.starts_at, &event.ends_at),
        status: event.status.clone(),
        visibility: event.visibility.clone(),
        signup_mode: event.signup_mode.clone(),
        display_capacity: event.display_capacity,
        public_confirmed: cap.public_confirmed,
        cap: cap.cap,
        signup_open_public,
        schedule: schedule
            .iter()
            .map(|s| ScheduleRow {
                title: s.title.clone(),
                when: format_optional_window(s.starts_at.as_deref(), s.ends_at.as_deref()),
                details: s.details.clone(),
                location_name: s.location_name.clone(),
            })
            .collect(),
        admin_bootstrap,
    };
    html(template.render())
}

pub(crate) fn render_rsvp(
    state: &AppState,
    event: &Event,
    invitee: &Invitee,
    guests: &[InviteeGuest],
    raw_token: &str,
) -> Response {
    let location = location_for_invitee(event, invitee);
    let bootstrap = json_attr(&RsvpBootstrap {
        event_ref: ref_for(event),
        token: raw_token.to_owned(),
        invitee: RsvpInvitee {
            display_name: invitee.display_name.clone(),
            party_size_limit: invitee.party_size_limit,
            rsvp_status: invitee.rsvp_status.clone(),
            arrival_note: invitee.arrival_note.clone(),
            dietary_restrictions: invitee.dietary_restrictions.clone(),
            general_notes: invitee.general_notes.clone(),
        },
        guests: guests
            .iter()
            .map(|g| RsvpGuest {
                id: g.id.clone(),
                display_name: g.display_name.clone(),
                attending: g.attending,
                dietary_restrictions: g.dietary_restrictions.clone(),
                general_notes: g.general_notes.clone(),
            })
            .collect(),
        event: RsvpEventMeta {
            title: event.title.clone(),
            starts_at: event.starts_at.clone(),
            location_name: location.name.clone(),
            notes_label: event.notes_label.clone(),
            notes_caption: event.notes_caption.clone(),
            dietary_label: event.dietary_label.clone(),
            arrival_note_label: event.arrival_note_label.clone(),
            arrival_note_caption: event.arrival_note_caption.clone(),
            allow_rsvp_edits: event.allow_rsvp_edits,
        },
    });
    let template = RsvpTemplate {
        script_src: state.manifest.entry("site.ts"),
        css_files: state.manifest.css_for_entry("site.ts"),
        event_title: event.title.clone(),
        invitee_name: invitee.display_name.clone(),
        starts_at: format_event_window(&event.starts_at, &event.ends_at),
        location_name: location.name,
        waitlisted: !invitee.location_approved,
        rsvp_closed: !event.allow_rsvp_edits,
        bootstrap,
    };
    html(template.render())
}

pub(crate) fn render_signup(
    state: &AppState,
    event: &Event,
    cap: &CapacityDisplay,
    signup_token: Option<&str>,
    signed_signup: bool,
) -> Response {
    let location = location_for_public(event);
    let event_ref = ref_for(event);
    let signup_allowed =
        (event.signup_mode == "self_signup" || signed_signup) && cap.self_signup_open;
    let bootstrap = json_attr(&SignupBootstrap {
        event_ref: event_ref.clone(),
        signup_token: signup_token.map(ToOwned::to_owned),
        event: RsvpEventMeta {
            title: event.title.clone(),
            starts_at: event.starts_at.clone(),
            location_name: location.name.clone(),
            notes_label: event.notes_label.clone(),
            notes_caption: event.notes_caption.clone(),
            dietary_label: event.dietary_label.clone(),
            arrival_note_label: event.arrival_note_label.clone(),
            arrival_note_caption: event.arrival_note_caption.clone(),
            allow_rsvp_edits: true,
        },
    });
    let template = SignupTemplate {
        script_src: state.manifest.entry("site.ts"),
        css_files: state.manifest.css_for_entry("site.ts"),
        event_title: event.title.clone(),
        starts_at: format_event_window(&event.starts_at, &event.ends_at),
        location_name: location.name,
        signup_allowed,
        capacity_reached: !cap.self_signup_open,
        public_confirmed: cap.public_confirmed,
        cap: cap.cap,
        bootstrap,
    };
    html(template.render())
}

fn ref_for(event: &Event) -> String {
    event.slug.clone().unwrap_or_else(|| event.id.clone())
}

struct LocationDisplay {
    name: Option<String>,
    address: Option<String>,
    map_url: Option<String>,
}

fn location_for_event(event: &Event, exact: bool) -> LocationDisplay {
    if exact {
        return LocationDisplay {
            name: event
                .location_name
                .clone()
                .or_else(|| event.approximate_location_name.clone()),
            address: event.address.clone(),
            map_url: event.map_url.clone(),
        };
    }
    location_for_public(event)
}

fn location_for_invitee(event: &Event, invitee: &Invitee) -> LocationDisplay {
    location_for_event(event, invitee.location_approved)
}

fn location_for_public(event: &Event) -> LocationDisplay {
    LocationDisplay {
        name: event.approximate_location_name.clone(),
        address: None,
        map_url: None,
    }
}

fn parse_rfc3339(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value).ok()
}

fn format_datetime_display(value: &str) -> String {
    parse_rfc3339(value)
        .map(|dt| format!("{} at {}", format_date(dt), format_time(dt, true)))
        .unwrap_or_else(|| value.to_owned())
}

fn format_event_window(starts_at: &str, ends_at: &str) -> String {
    let Some(start) = parse_rfc3339(starts_at) else {
        return if ends_at.is_empty() {
            starts_at.to_owned()
        } else {
            format!("{starts_at} - {ends_at}")
        };
    };
    let Some(end) = parse_rfc3339(ends_at) else {
        return format_datetime_display(starts_at);
    };
    format_window(start, end)
}

fn format_optional_window(starts_at: Option<&str>, ends_at: Option<&str>) -> Option<String> {
    match (starts_at, ends_at) {
        (Some(start), Some(end)) => Some(format_event_window(start, end)),
        (Some(start), None) => Some(format_datetime_display(start)),
        (None, Some(end)) => Some(format_datetime_display(end)),
        (None, None) => None,
    }
}

fn format_window(start: DateTime<FixedOffset>, end: DateTime<FixedOffset>) -> String {
    let end_in_start_offset = end.with_timezone(start.offset());
    if start.date_naive() == end_in_start_offset.date_naive() {
        return format!(
            "{} at {}",
            format_date(start),
            format_time_range(start, end_in_start_offset)
        );
    }
    format!(
        "{} at {} - {} at {}",
        format_date(start),
        format_time(start, true),
        format_date(end_in_start_offset),
        format_time(end_in_start_offset, true)
    )
}

fn format_date(dt: DateTime<FixedOffset>) -> String {
    const WEEKDAYS: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let weekday = WEEKDAYS[dt.weekday().num_days_from_monday() as usize];
    let month = MONTHS[dt.month0() as usize];
    let mut out = format!("{weekday} {month} {}{}", dt.day(), ordinal_suffix(dt.day()));
    if show_year(dt) {
        out.push_str(&format!(", {}", dt.year()));
    }
    out
}

fn show_year(dt: DateTime<FixedOffset>) -> bool {
    let days = dt
        .with_timezone(&Utc)
        .signed_duration_since(Utc::now())
        .num_days()
        .abs();
    days > 92
}

fn ordinal_suffix(day: u32) -> &'static str {
    if (11..=13).contains(&(day % 100)) {
        return "th";
    }
    match day % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

fn format_time_range(start: DateTime<FixedOffset>, end: DateTime<FixedOffset>) -> String {
    let same_suffix = meridiem(start) == meridiem(end);
    if same_suffix {
        format!("{}-{}", format_time(start, false), format_time(end, true))
    } else {
        format!("{}-{}", format_time(start, true), format_time(end, true))
    }
}

fn format_time(dt: DateTime<FixedOffset>, suffix: bool) -> String {
    let hour = dt.hour();
    let hour12 = match hour % 12 {
        0 => 12,
        value => value,
    };
    let minute = dt.minute();
    let mut out = if minute == 0 {
        hour12.to_string()
    } else {
        format!("{hour12}:{minute:02}")
    };
    if suffix {
        out.push_str(meridiem(dt));
    }
    out
}

fn meridiem(dt: DateTime<FixedOffset>) -> &'static str {
    if dt.hour() < 12 { "am" } else { "pm" }
}

fn json_attr<T: Serialize>(value: &T) -> String {
    // Return raw JSON; Askama's default escaper handles HTML attribute context.
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

fn html(result: Result<String, askama::Error>) -> Response {
    match result {
        Ok(body) => Html(body).into_response(),
        Err(err) => {
            tracing::error!(?err, "template render failed");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "template error",
            )
                .into_response()
        }
    }
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    script_src: Option<String>,
    css_files: Vec<String>,
}

#[derive(Template)]
#[template(path = "events.html")]
struct EventsListTemplate {
    script_src: Option<String>,
    css_files: Vec<String>,
    is_admin: bool,
    events: Vec<EventsListRow>,
    admin_bootstrap: Option<String>,
}

struct EventsListRow {
    ref_: String,
    title: String,
    subtitle: Option<String>,
    summary: Option<String>,
    starts_at: String,
    location_name: Option<String>,
    status: String,
    visibility: String,
    signup_mode: String,
}

#[derive(Template)]
#[template(path = "event_detail.html")]
struct EventDetailTemplate {
    script_src: Option<String>,
    css_files: Vec<String>,
    is_admin: bool,
    event_ref: String,
    title: String,
    subtitle: Option<String>,
    summary: Option<String>,
    details_markdown: String,
    location_name: Option<String>,
    address: Option<String>,
    map_url: Option<String>,
    when: String,
    status: String,
    visibility: String,
    signup_mode: String,
    display_capacity: bool,
    public_confirmed: i64,
    cap: Option<i64>,
    signup_open_public: bool,
    schedule: Vec<ScheduleRow>,
    admin_bootstrap: Option<String>,
}

struct ScheduleRow {
    title: String,
    when: Option<String>,
    details: Option<String>,
    location_name: Option<String>,
}

#[derive(Template)]
#[template(path = "event_rsvp.html")]
struct RsvpTemplate {
    script_src: Option<String>,
    css_files: Vec<String>,
    event_title: String,
    invitee_name: String,
    starts_at: String,
    location_name: Option<String>,
    waitlisted: bool,
    rsvp_closed: bool,
    bootstrap: String,
}

#[derive(Template)]
#[template(path = "event_signup.html")]
struct SignupTemplate {
    script_src: Option<String>,
    css_files: Vec<String>,
    event_title: String,
    starts_at: String,
    location_name: Option<String>,
    signup_allowed: bool,
    capacity_reached: bool,
    public_confirmed: i64,
    cap: Option<i64>,
    bootstrap: String,
}

#[derive(Serialize)]
struct AdminListBootstrap {}

#[derive(Serialize)]
struct AdminEventBootstrap {
    event_ref: String,
    event_id: String,
    confirmed: i64,
    over_cap: i64,
    public_confirmed: i64,
    cap: Option<i64>,
    title: String,
    subtitle: Option<String>,
    summary: Option<String>,
    details_markdown: String,
    approximate_location_name: Option<String>,
    location_name: Option<String>,
    address: Option<String>,
    map_url: Option<String>,
    display_capacity: bool,
    self_signup_requires_approval: bool,
    notes_label: String,
    notes_caption: Option<String>,
    dietary_label: String,
    arrival_note_label: String,
    arrival_note_caption: Option<String>,
    allow_rsvp_edits: bool,
    status: String,
    visibility: String,
    signup_mode: String,
    starts_at: String,
    ends_at: String,
    timezone: String,
    attendee_cap: Option<i64>,
    self_signup_token_set: bool,
    invitees: Vec<AdminInviteeRow>,
}

#[derive(Serialize)]
struct AdminInviteeRow {
    id: String,
    display_name: String,
    email: Option<String>,
    rsvp_status: String,
    party_size_limit: i64,
    location_approved: bool,
    opened_at: Option<String>,
    responded_at: Option<String>,
}

#[derive(Serialize)]
struct RsvpBootstrap {
    event_ref: String,
    token: String,
    invitee: RsvpInvitee,
    guests: Vec<RsvpGuest>,
    event: RsvpEventMeta,
}

#[derive(Serialize)]
struct RsvpInvitee {
    display_name: String,
    party_size_limit: i64,
    rsvp_status: String,
    arrival_note: String,
    dietary_restrictions: String,
    general_notes: String,
}

#[derive(Serialize)]
struct RsvpGuest {
    id: String,
    display_name: String,
    attending: bool,
    dietary_restrictions: String,
    general_notes: String,
}

#[derive(Serialize)]
struct RsvpEventMeta {
    title: String,
    starts_at: String,
    location_name: Option<String>,
    notes_label: String,
    notes_caption: Option<String>,
    dietary_label: String,
    arrival_note_label: String,
    arrival_note_caption: Option<String>,
    allow_rsvp_edits: bool,
}

#[derive(Serialize)]
struct SignupBootstrap {
    event_ref: String,
    signup_token: Option<String>,
    event: RsvpEventMeta,
}
