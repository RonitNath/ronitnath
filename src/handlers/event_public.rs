//! The guest surface: everything reachable through a capability link.
//!
//! `GET /e/{token}` is the capability entry surface. Audience policy decides
//! the computed level; only the direct-hit policy may apply the link-tier
//! floors (public → Summary, private → Full).
//! - a link with a `person_id` additionally greets that person and lets
//!   them edit their own RSVP; a shared link asks for a name and mints the
//!   submitter a personal link on first RSVP.
//!
//! Visibility computation is centralized in `access::level`; event and
//! schedule redaction remain their respective chokepoints.

use askama::Template;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utoipa::ToSchema;

use crate::access::level::Level;
use crate::auth::csrf;
use crate::auth::extract::{NavContext, NavUser};
use crate::auth::session::hash_token;
use crate::auth::viewer::Viewer;
use crate::dates as filters;
use crate::error::AppError;
use crate::state::AppState;
use crate::store::Store;
use crate::store::attendance::Attendance;
use crate::store::event_links::ResolvedLink;
use crate::store::event_links::personal_token;
use crate::store::events::{Event, EventView};
use crate::store::schedule_items::ScheduleItem;
use crate::store::segment_rsvps::{SegmentCount, SegmentRsvp};
use crate::store::sessions::SessionContext;
use crate::view::render;

const MAX_NAME_LEN: usize = 100;
const MAX_NOTE_LEN: usize = 500;
const MAX_PARTY_SIZE: i64 = 10;

/// The personalized slice of a guest view.
#[derive(Debug, Serialize, TS, ToSchema)]
#[ts(export)]
pub struct GuestPerson {
    pub name: String,
    pub attendance: Option<Attendance>,
    pub segments: Vec<SegmentRsvp>,
}

/// Everything the RSVP island needs, level-filtered server-side.
#[derive(Debug, Serialize, TS, ToSchema)]
#[ts(export)]
pub struct GuestView {
    pub event: EventView,
    pub schedule: Vec<ScheduleItem>,
    pub segment_counts: Vec<SegmentCount>,
    pub person: Option<GuestPerson>,
}

#[derive(Debug, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct SegmentChoice {
    #[ts(type = "number")]
    pub schedule_item_id: i64,
    /// `in` | `maybe` | `out`
    pub status: String,
}

#[derive(Debug, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct RsvpSubmit {
    /// Required on shared links (that's how a person comes to exist);
    /// ignored on personalized links.
    pub name: Option<String>,
    /// `going` | `maybe` | `no`
    pub status: String,
    #[ts(type = "number")]
    pub party_size: i64,
    pub note: String,
    pub segments: Vec<SegmentChoice>,
}

#[derive(Debug, Serialize, TS, ToSchema)]
#[ts(export)]
pub struct RsvpResult {
    pub person_name: String,
    /// Set when this RSVP just minted the submitter their own link (shared
    /// link → first RSVP). The island shows it as "save this link".
    pub personal_url: Option<String>,
}

/// Resolves a raw token or answers 404. Unknown and revoked tokens are
/// indistinguishable by design.
pub(crate) async fn resolve(store: &Store, token: &str) -> Result<(ResolvedLink, Event), AppError> {
    let link = store
        .resolve_event_link(&hash_token(token))
        .await?
        .ok_or(AppError::NotFound)?;
    let event = store
        .find_event(link.account_id, link.event_id)
        .await?
        .ok_or(AppError::NotFound)?;
    // Draft events are invisible even through a live link.
    if event.status == "draft" {
        return Err(AppError::NotFound);
    }
    Ok((link, event))
}

async fn direct_level(
    store: &Store,
    link: &ResolvedLink,
    viewer: &Viewer,
) -> Result<Level, AppError> {
    let inputs = store
        .audience_inputs_for_event(link.account_id, link.event_id, viewer.person_id())
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(inputs.level_for_direct_hit(viewer, &link.tier)?)
}

pub(crate) async fn build_view(
    store: &Store,
    link: &ResolvedLink,
    event: &Event,
    level: Level,
) -> Result<GuestView, AppError> {
    let schedule = store
        .list_schedule(link.account_id, event.id, level)
        .await?;
    let segment_counts = store
        .segment_counts(link.account_id, event.id, level)
        .await?;

    let person = match link.person_id {
        Some(person_id) => match store.find_person(link.account_id, person_id).await? {
            Some(p) => Some(GuestPerson {
                name: p.name,
                attendance: store
                    .find_attendance(link.account_id, event.id, person_id)
                    .await?,
                segments: store
                    .list_segment_rsvps_for_person(link.account_id, event.id, person_id, level)
                    .await?,
            }),
            None => None,
        },
        None => None,
    };

    Ok(GuestView {
        event: event.view_for(level).ok_or(AppError::NotFound)?,
        schedule,
        segment_counts,
        person,
    })
}

// Standalone template (doesn't extend `_layout.html`): the guest page has
// no nav, no auth widget, no admin affordances — a guest sees only the
// event. It still includes `_theme.html` so the CSP inline-script hash and
// theming keep working.
#[derive(Template)]
#[template(path = "event_public.html")]
struct EventPublicTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    rsvp_endpoint: String,
    poster_theme: bool,
    view: GuestView,
    mismatch_note: Option<String>,
}

/// The guest event page. Server-renders the level-appropriate info so the
/// page is fully readable without JS; the RSVP island hydrates on top.
pub async fn page(
    State(state): State<AppState>,
    session_viewer: Viewer,
    NavContext(current_user): NavContext,
    Path(token): Path<String>,
) -> Result<Response, AppError> {
    let (link, event) = resolve(state.store(), &token).await?;
    let (viewer, mismatch) = session_viewer.combine_with_link(Some(&link));
    let level = direct_level(state.store(), &link, &viewer).await?;
    let view = build_view(state.store(), &link, &event, level).await?;
    let mismatch_note = if let Some(note) = mismatch {
        let signed_in = state
            .store()
            .find_person(link.account_id, note.guest_person_id)
            .await?
            .map(|p| p.name)
            .unwrap_or_else(|| "another guest".into());
        let link_person = match link.person_id {
            Some(id) => state
                .store()
                .find_person(link.account_id, id)
                .await?
                .map(|p| p.name)
                .unwrap_or_else(|| "this guest".into()),
            None => "a shared link".into(),
        };
        Some(format!(
            "Viewing as {link_person}; signed in as {signed_in}"
        ))
    } else {
        None
    };
    render(EventPublicTemplate {
        nav_active: "events",
        current_user,
        rsvp_endpoint: format!("/api/e/{token}"),
        poster_theme: event.slug == "july4-2026",
        view,
        mismatch_note,
    })
}

/// Calendar export through the same live capability-link policy as the page.
/// The computed level controls location redaction via `Event::view_for`.
pub async fn ics(
    State(state): State<AppState>,
    session_viewer: Viewer,
    Path(event_ref): Path<String>,
) -> Result<Response, AppError> {
    let (link, event) = resolve(state.store(), &event_ref).await?;
    let (viewer, _) = session_viewer.combine_with_link(Some(&link));
    let level = direct_level(state.store(), &link, &viewer).await?;
    let _schedule = state
        .store()
        .list_schedule(link.account_id, event.id, level)
        .await?;
    let view = event.view_for(level).ok_or(AppError::NotFound)?;
    let crate::store::events::EventView::Event(view) = view else {
        return Err(AppError::NotFound);
    };
    let location = view.address.as_deref().unwrap_or(&view.area_name);
    let end = view.ends_at.as_deref().unwrap_or(&view.starts_at);
    let body = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//ronitnath//events//EN\r\nBEGIN:VEVENT\r\nUID:{}@ronitnath.com\r\nSUMMARY:{}\r\nDTSTART:{}\r\nDTEND:{}\r\nLOCATION:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        event.id,
        escape_ics(&view.title),
        ics_datetime(&view.starts_at),
        ics_datetime(end),
        escape_ics(location),
    );
    Ok((
        [(header::CONTENT_TYPE, "text/calendar; charset=utf-8")],
        body,
    )
        .into_response())
}

fn ics_datetime(value: &str) -> String {
    let compact: String = value
        .chars()
        .filter(|c| !matches!(c, '-' | ':'))
        .map(|c| if c == ' ' { 'T' } else { c })
        .collect();
    if compact.len() == 13 {
        format!("{compact}00")
    } else {
        compact
    }
}

fn escape_ics(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
        .replace('\n', "\\n")
        .replace('\r', "")
}

#[utoipa::path(
    get,
    path = "/api/e/{token}",
    tag = "events-guest",
    responses(
        (status = 200, description = "Tier-filtered event view for this link", body = GuestView),
        (status = 404, description = "Unknown or revoked link"),
    )
)]
pub async fn api_view(
    State(state): State<AppState>,
    session_viewer: Viewer,
    Path(token): Path<String>,
) -> Result<Json<GuestView>, AppError> {
    let (link, event) = resolve(state.store(), &token).await?;
    let (viewer, _) = session_viewer.combine_with_link(Some(&link));
    let level = direct_level(state.store(), &link, &viewer).await?;
    Ok(Json(build_view(state.store(), &link, &event, level).await?))
}

#[utoipa::path(
    post,
    path = "/api/e/{token}/rsvp",
    tag = "events-guest",
    request_body = RsvpSubmit,
    responses(
        (status = 200, description = "RSVP stored", body = RsvpResult),
        (status = 404, description = "Unknown or revoked link"),
        (status = 422, description = "Invalid RSVP payload"),
    )
)]
pub async fn api_rsvp(
    State(state): State<AppState>,
    session_viewer: Viewer,
    Extension(session_ctx): Extension<Option<SessionContext>>,
    Path(token): Path<String>,
    headers: HeaderMap,
    Json(submit): Json<RsvpSubmit>,
) -> Result<Json<RsvpResult>, AppError> {
    let store = state.store();
    let (link, event) = resolve(store, &token).await?;
    let session_guest = match &session_viewer {
        Viewer::Guest {
            identity_id,
            person_id,
        } => Some((*identity_id, *person_id)),
        _ => None,
    };
    if session_guest.is_some() {
        let submitted_csrf = headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let expected_csrf = session_ctx
            .as_ref()
            .map(|ctx| ctx.csrf_token.as_str())
            .ok_or_else(|| AppError::Forbidden("missing or invalid CSRF token".into()))?;
        csrf::verify_optional(Some(expected_csrf), submitted_csrf)?;
    }
    let (viewer, _) = session_viewer.combine_with_link(Some(&link));
    let level = direct_level(store, &link, &viewer).await?;
    if event.view_for(level).is_none() {
        return Err(AppError::NotFound);
    }

    if !matches!(submit.status.as_str(), "going" | "maybe" | "no") {
        return Err(AppError::Invalid(
            "status must be going, maybe, or no".into(),
        ));
    }
    if !(1..=MAX_PARTY_SIZE).contains(&submit.party_size) {
        return Err(AppError::Invalid(format!(
            "party size must be between 1 and {MAX_PARTY_SIZE}"
        )));
    }
    if submit.note.len() > MAX_NOTE_LEN {
        return Err(AppError::Invalid(format!(
            "note must be under {MAX_NOTE_LEN} characters"
        )));
    }

    // Segment choices may only target RSVP-able items this viewer can see;
    // list_schedule is the schedule redaction chokepoint.
    let visible = store
        .list_schedule(link.account_id, event.id, level)
        .await?;
    for choice in &submit.segments {
        if !matches!(choice.status.as_str(), "in" | "maybe" | "out") {
            return Err(AppError::Invalid(
                "segment status must be in, maybe, or out".into(),
            ));
        }
        if !visible
            .iter()
            .any(|item| item.id == choice.schedule_item_id && item.segment_key.is_some())
        {
            return Err(AppError::Invalid("unknown schedule segment".into()));
        }
    }

    // A live guest session authenticates the write even when content is
    // being rendered through somebody else's token. Anonymous requests keep
    // capability attribution: bound links write their person; shared links
    // create a person and personal return link.
    let (person_id, person_name, personal_url) = match (session_guest, link.person_id) {
        (Some((_, person_id)), _) | (None, Some(person_id)) => {
            let person = store
                .find_person(link.account_id, person_id)
                .await?
                .ok_or(AppError::NotFound)?;
            (person_id, person.name, None)
        }
        (None, None) => {
            let name = submit.name.as_deref().map(str::trim).unwrap_or_default();
            if name.is_empty() {
                return Err(AppError::Invalid("please tell us your name".into()));
            }
            if name.len() > MAX_NAME_LEN {
                return Err(AppError::Invalid(format!(
                    "name must be under {MAX_NAME_LEN} characters"
                )));
            }
            let person = store.create_person(link.account_id, name, "").await?;
            let raw = personal_token(name);
            store
                .create_event_link(
                    link.account_id,
                    event.id,
                    Some(person.id),
                    &hash_token(&raw),
                    &raw,
                    "self-signup",
                    &link.tier,
                )
                .await?;
            let url = format!("{}/e/{}", state.public_url(), raw);
            (person.id, person.name, Some(url))
        }
    };

    store
        .upsert_attendance(
            link.account_id,
            event.id,
            person_id,
            &submit.status,
            submit.party_size,
            submit.note.trim(),
        )
        .await?;
    for choice in &submit.segments {
        store
            .upsert_segment_rsvp(
                link.account_id,
                choice.schedule_item_id,
                person_id,
                &choice.status,
            )
            .await?;
    }

    store
        .audit(
            session_guest.map(|(identity_id, _)| identity_id),
            Some(link.account_id),
            None,
            "event.rsvp",
            "event",
            Some(&event.id.to_string()),
            &serde_json::json!({
                "person_id": person_id,
                "status": submit.status,
                "party_size": submit.party_size,
                "via_link": link.id,
                "via_session": session_guest.is_some(),
            }),
        )
        .await?;

    Ok(Json(RsvpResult {
        person_name,
        personal_url,
    }))
}
