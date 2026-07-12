//! The admin operations surface for events: create events, see overview
//! activity, mint/revoke capability links, bulk-add guests, and make minor
//! person/attendance corrections. Event copy and schedule edits are agent/CLI
//! territory, not web forms.

use std::collections::HashMap;

use askama::Template;
use axum::Form;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use qrcode::QrCode;
use qrcode::render::svg;
use serde::Deserialize;

use crate::access::level::Level;
use crate::auth::extract::NavUser;
use crate::auth::session::{generate_token, hash_token};
use crate::auth::{AccountScope, Role, csrf};
use crate::dates as filters;
use crate::error::AppError;
use crate::state::AppState;
use crate::store::attendance::AttendanceRow;
use crate::store::event_links::EventLinkRow;
use crate::store::events::Event;
use crate::store::people::Person;
use crate::store::schedule_items::ScheduleItem;
use crate::view::render;

fn nav_user(scope: &AccountScope) -> Option<NavUser> {
    Some(NavUser {
        display_name: scope.display_name.clone(),
        csrf_token: scope.csrf_token.clone().unwrap_or_default(),
        is_guest: false,
    })
}

// ---------- /events (list) ----------

struct EventListRow {
    event: Event,
    going: i64,
    maybe: i64,
    heads: i64,
}

#[derive(Template)]
#[template(path = "events/list.html")]
struct EventListTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    rows: Vec<EventListRow>,
}

pub async fn list_page(
    State(state): State<AppState>,
    scope: AccountScope,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let events = state.store().list_events(scope.account_id).await?;
    let mut rows = Vec::with_capacity(events.len());
    for event in events {
        let (going, maybe, heads) = state
            .store()
            .attendance_counts(scope.account_id, event.id)
            .await?;
        rows.push(EventListRow {
            event,
            going,
            maybe,
            heads,
        });
    }
    render(EventListTemplate {
        nav_active: "events",
        current_user: nav_user(&scope),
        csrf_token: scope.csrf_token.unwrap_or_default(),
        rows,
    })
}

#[derive(Deserialize)]
pub struct CreateEventForm {
    slug: String,
    title: String,
    starts_at: String,
    csrf_token: String,
}

pub async fn create_event(
    State(state): State<AppState>,
    scope: AccountScope,
    Form(form): Form<CreateEventForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    let slug = form.slug.trim().to_lowercase();
    if slug.is_empty() || form.title.trim().is_empty() || form.starts_at.trim().is_empty() {
        return Err(AppError::Invalid(
            "slug, title, and start time are required".into(),
        ));
    }
    let event = state
        .store()
        .create_event(
            scope.account_id,
            &slug,
            form.title.trim(),
            form.starts_at.trim(),
        )
        .await?;
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "event.created",
            "event",
            Some(&event.id.to_string()),
            &serde_json::json!({ "slug": slug }),
        )
        .await?;
    Ok(Redirect::to(&format!("/events/{}", event.id)).into_response())
}

// ---------- /events/{id} (overview) ----------

struct SegmentOverview {
    schedule_item_id: i64,
    time_label: String,
    title: String,
    tier: String,
    segment_key: String,
    in_count: i64,
    maybe_count: i64,
}

struct EventLinkOverview {
    row: EventLinkRow,
    url: String,
    invite_text: Option<String>,
    qr_svg: String,
    never_used_personal: bool,
}

#[derive(Template)]
#[template(path = "events/detail.html")]
struct EventDetailTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    event: Event,
    segments: Vec<SegmentOverview>,
    links: Vec<EventLinkOverview>,
    attendance: Vec<AttendanceRow>,
    people: Vec<Person>,
    going: i64,
    maybe: i64,
    heads: i64,
    photos: Vec<crate::handlers::photos::GalleryPhoto>,
}

pub async fn detail_page(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(event_id): Path<i64>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let store = state.store();
    let event = store
        .find_event(scope.account_id, event_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let schedule = store
        .list_schedule(scope.account_id, event_id, Level::Full)
        .await?;
    let segment_counts = store
        .segment_counts(scope.account_id, event_id, Level::Full)
        .await?;
    let raw_links = store.list_event_links(scope.account_id, event_id).await?;
    let attendance = store.list_attendance(scope.account_id, event_id).await?;
    let people = store.list_people(scope.account_id).await?;
    let (going, maybe, heads) = store.attendance_counts(scope.account_id, event_id).await?;
    let photo_viewer = crate::auth::viewer::Viewer::Owner { identity_id: scope.identity_id };
    let photos = crate::handlers::photos::gallery(
        &state, scope.account_id, event_id, &photo_viewer,
        &format!("/events/{event_id}/photos"),
    ).await?;

    let count_by_item: HashMap<i64, (i64, i64)> = segment_counts
        .into_iter()
        .map(|count| (count.schedule_item_id, (count.in_count, count.maybe_count)))
        .collect();
    let segments = segment_overviews(&schedule, &count_by_item);
    let links = link_overviews(raw_links, state.public_url(), &event)?;

    render(EventDetailTemplate {
        nav_active: "events",
        current_user: nav_user(&scope),
        csrf_token: scope.csrf_token.unwrap_or_default(),
        event,
        segments,
        links,
        attendance,
        people,
        going,
        maybe,
        heads,
        photos,
    })
}

fn segment_overviews(
    schedule: &[ScheduleItem],
    count_by_item: &HashMap<i64, (i64, i64)>,
) -> Vec<SegmentOverview> {
    schedule
        .iter()
        .filter_map(|item| {
            let segment_key = item.segment_key.as_ref()?;
            let (in_count, maybe_count) = count_by_item.get(&item.id).copied().unwrap_or((0, 0));
            Some(SegmentOverview {
                schedule_item_id: item.id,
                time_label: item.time_label.clone(),
                title: item.title.clone(),
                tier: item.tier.clone(),
                segment_key: segment_key.clone(),
                in_count,
                maybe_count,
            })
        })
        .collect()
}

fn link_overviews(
    links: Vec<EventLinkRow>,
    public_url: &str,
    event: &Event,
) -> Result<Vec<EventLinkOverview>, AppError> {
    links
        .into_iter()
        .map(|row| {
            let url = format!("{public_url}/e/{}", row.token_plain);
            let invite_text = row.person_name.as_ref().map(|name| {
                format!(
                    "Hey {name}! You're invited to {} — {}. All the details + RSVP here: {url}",
                    event.title, event.starts_at,
                )
            });
            let qr_svg = qr_svg(&url)?;
            let never_used_personal =
                row.person_id.is_some() && row.uses == 0 && row.revoked_at.is_none();
            Ok(EventLinkOverview {
                row,
                url,
                invite_text,
                qr_svg,
                never_used_personal,
            })
        })
        .collect()
}

fn qr_svg(url: &str) -> Result<String, AppError> {
    let code = QrCode::new(url.as_bytes()).map_err(|err| anyhow::anyhow!(err))?;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(96, 96)
        .dark_color(svg::Color("#111111"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

// ---------- attendance override ----------

#[derive(Deserialize)]
pub struct AttendanceOverrideForm {
    status: String,
    party_size: i64,
    csrf_token: String,
}

pub async fn update_attendance(
    State(state): State<AppState>,
    scope: AccountScope,
    Path((event_id, person_id)): Path<(i64, i64)>,
    Form(form): Form<AttendanceOverrideForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    if !matches!(
        form.status.as_str(),
        "none" | "going" | "maybe" | "no" | "attended"
    ) {
        return Err(AppError::Invalid("bad attendance status".into()));
    }
    if form.party_size < 1 {
        return Err(AppError::Invalid("party size must be at least 1".into()));
    }
    let store = state.store();
    store
        .find_event(scope.account_id, event_id)
        .await?
        .ok_or(AppError::NotFound)?;
    store
        .find_person(scope.account_id, person_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let note = store
        .find_attendance(scope.account_id, event_id, person_id)
        .await?
        .map_or(String::new(), |attendance| attendance.note);
    store
        .upsert_attendance(
            scope.account_id,
            event_id,
            person_id,
            &form.status,
            form.party_size,
            &note,
        )
        .await?;
    store
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "attendance.updated",
            "attendance",
            Some(&format!("{event_id}:{person_id}")),
            &serde_json::json!({ "event_id": event_id, "person_id": person_id, "status": form.status, "party_size": form.party_size }),
        )
        .await?;
    Ok(Redirect::to(&format!("/events/{event_id}#rsvps")).into_response())
}

// ---------- links ----------

#[derive(Deserialize)]
pub struct CreateLinkForm {
    label: String,
    tier: String,
    /// Empty string = shareable link; otherwise exact-name match or create.
    person_name: String,
    csrf_token: String,
}

pub async fn create_link(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(event_id): Path<i64>,
    Form(form): Form<CreateLinkForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    if !matches!(form.tier.as_str(), "public" | "private") {
        return Err(AppError::Invalid("tier must be public or private".into()));
    }
    let store = state.store();
    store
        .find_event(scope.account_id, event_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let person_id = match form.person_name.trim() {
        "" => None,
        name => Some(
            match store.find_person_by_name(scope.account_id, name).await? {
                Some(person) => person.id,
                None => store.create_person(scope.account_id, name, "").await?.id,
            },
        ),
    };

    // Personalized links get the friendly name-suffix form; shareable
    // links keep the long random token.
    let raw = match form.person_name.trim() {
        "" => generate_token(),
        name => crate::store::event_links::personal_token(name),
    };
    let link_id = store
        .create_event_link(
            scope.account_id,
            event_id,
            person_id,
            &hash_token(&raw),
            &raw,
            form.label.trim(),
            &form.tier,
        )
        .await?;
    store
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "event.link_created",
            "event_link",
            Some(&link_id.to_string()),
            &serde_json::json!({ "event_id": event_id, "tier": form.tier, "person_id": person_id }),
        )
        .await?;
    Ok(Redirect::to(&format!("/events/{event_id}#links")).into_response())
}

#[derive(Deserialize)]
pub struct CsrfOnlyForm {
    csrf_token: String,
}

pub async fn revoke_link(
    State(state): State<AppState>,
    scope: AccountScope,
    Path((event_id, link_id)): Path<(i64, i64)>,
    Form(form): Form<CsrfOnlyForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    let revoked = state
        .store()
        .revoke_event_link(scope.account_id, link_id)
        .await?;
    if revoked == 0 {
        return Err(AppError::NotFound);
    }
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "event.link_revoked",
            "event_link",
            Some(&link_id.to_string()),
            &serde_json::json!({ "event_id": event_id }),
        )
        .await?;
    Ok(Redirect::to(&format!("/events/{event_id}#links")).into_response())
}

// ---------- guests ----------

#[derive(Deserialize)]
pub struct BulkAddForm {
    /// One person per line: `Name` or `Name | group`.
    names: String,
    /// Attendance status to stamp: `none` for fresh invites, `attended`
    /// when backfilling a past event's guest list.
    status: String,
    /// `1` mints a personalized private link per person (skip for backfill).
    mint_links: Option<String>,
    csrf_token: String,
}

pub async fn bulk_add_people(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(event_id): Path<i64>,
    Form(form): Form<BulkAddForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    if !matches!(
        form.status.as_str(),
        "none" | "going" | "maybe" | "attended"
    ) {
        return Err(AppError::Invalid("bad attendance status".into()));
    }
    let store = state.store();
    store
        .find_event(scope.account_id, event_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Reuse an existing person on exact name match (case-insensitive) so
    // backfilling past events links to the same person rows — that's the
    // longitudinal point. New names create new people.
    let existing = store.list_people(scope.account_id).await?;
    let mint = form.mint_links.as_deref() == Some("1");

    for line in form.names.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (name, group) = match line.split_once('|') {
            Some((n, g)) => (n.trim(), g.trim()),
            None => (line, ""),
        };
        if name.is_empty() {
            continue;
        }

        let person_id = match existing.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
            Some(p) => p.id,
            None => store.create_person(scope.account_id, name, group).await?.id,
        };

        store
            .upsert_attendance(scope.account_id, event_id, person_id, &form.status, 1, "")
            .await?;

        if mint
            && store
                .find_personal_link(scope.account_id, event_id, person_id)
                .await?
                .is_none()
        {
            let raw = generate_token();
            store
                .create_event_link(
                    scope.account_id,
                    event_id,
                    Some(person_id),
                    &hash_token(&raw),
                    &raw,
                    "invite",
                    "private",
                )
                .await?;
        }
    }

    Ok(Redirect::to(&format!("/events/{event_id}#rsvps")).into_response())
}
