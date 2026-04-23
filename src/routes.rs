#![allow(clippy::result_large_err)]

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::auth::SessionData;
use crate::events::capacity;
use crate::events::errors::EventError;
use crate::events::models::{
    CreateEvent, CreateInvitee, CreateScript, Event, EventStatus, RsvpUpdate, UpdateEvent,
};
use crate::events::qr;
use crate::events::service::SelfSignupInput;
use crate::events::viewer::{Viewer, viewer_from_session};
use crate::{AppState, render};

const DEV_ADMIN_COOKIE: &str = "__rn_dev_admin";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/events", get(events_html).post(create_event))
        .route("/events.json", get(events_json))
        .route(
            "/events/{event_ref}",
            get(event_detail_html).post(update_event),
        )
        .route("/api/events/{event_ref}", get(event_json))
        .route("/events/{event_ref}/publish", post(publish_event))
        .route("/events/{event_ref}/archive", post(archive_event))
        .route(
            "/events/{event_ref}/capacity.json",
            get(event_capacity_json),
        )
        .route("/events/{event_ref}/ics", get(event_ics))
        .route("/events/{event_ref}/schedule", post(create_schedule_item))
        .route("/events/{event_ref}/invitees", post(create_invitee))
        .route("/events/{event_ref}/invitees.json", get(list_invitees_json))
        .route(
            "/events/{event_ref}/invitees/{invitee_id}/approve",
            post(approve_invitee),
        )
        .route(
            "/events/{event_ref}/invitees/{invitee_id}/regenerate",
            post(regenerate_invitee_token),
        )
        .route("/events/{event_ref}/scripts", post(create_script))
        .route(
            "/events/{event_ref}/scripts/{script_id}/render",
            post(render_script_json),
        )
        .route(
            "/events/{event_ref}/signup-token",
            post(create_signup_token),
        )
        .route(
            "/events/{event_ref}/signup",
            get(signup_html).post(self_signup),
        )
        .route("/events/{event_ref}/qr/signup.svg", get(signup_qr))
        .route(
            "/events/{event_ref}/r/{token}",
            get(rsvp_html).post(update_rsvp),
        )
        .route("/api/events/{event_ref}/r/{token}", get(invitee_by_token))
        .route("/enter", get(enter))
        .route("/dev/login", get(dev_login))
        .route("/dev/logout", get(dev_logout))
        .route("/healthz", get(healthz))
}

async fn home(State(state): State<AppState>) -> Response {
    render::render_home(&state)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn enter(State(state): State<AppState>) -> Response {
    if state.dev_mode {
        return Redirect::temporary("/dev/login").into_response();
    }
    if state.auth_ready {
        Redirect::temporary("/auth/login?return_to=%2Fevents").into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication is not available",
        )
            .into_response()
    }
}

async fn dev_login(State(state): State<AppState>) -> Response {
    if !state.dev_mode {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let cookie = format!("{DEV_ADMIN_COOKIE}=1; Path=/; Max-Age=86400; SameSite=Lax");
    (
        [(header::SET_COOKIE, cookie)],
        Redirect::temporary("/events"),
    )
        .into_response()
}

async fn dev_logout(State(state): State<AppState>) -> Response {
    if !state.dev_mode {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let cookie = format!("{DEV_ADMIN_COOKIE}=; Path=/; Max-Age=0; SameSite=Lax");
    (
        [(header::SET_COOKIE, cookie)],
        Redirect::temporary("/events"),
    )
        .into_response()
}

async fn events_html(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
) -> Response {
    let viewer = viewer(&state, session.as_ref(), &headers);
    let events = match state.events.list_events(&viewer).await {
        Ok(v) => v,
        Err(err) => return event_error(err),
    };
    render::render_events_list(&state, &viewer, &events)
}

async fn events_json(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, Response> {
    let viewer = viewer(&state, session.as_ref(), &headers);
    let events = state
        .events
        .list_events(&viewer)
        .await
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({
        "viewer": viewer,
        "events": events,
    })))
}

async fn event_detail_html(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
) -> Response {
    let viewer = viewer(&state, session.as_ref(), &headers);
    let event = match state.events.get_event(&event_ref, &viewer).await {
        Ok(e) => e,
        Err(err) => return event_error(err),
    };
    let schedule = match state.events.store().schedule_for_event(&event.id).await {
        Ok(s) => s,
        Err(err) => return event_error(EventError::from(err)),
    };
    let cap = match capacity::display(state.events.store(), &event).await {
        Ok(c) => c,
        Err(err) => return event_error(err),
    };
    let admin_extras = if viewer.is_admin() {
        let invitees = match state.events.store().invitees_for_event(&event.id).await {
            Ok(v) => v,
            Err(err) => return event_error(EventError::from(err)),
        };
        Some(render::AdminExtras {
            confirmed: cap.confirmed,
            over_cap: cap.over_cap,
            invitees,
        })
    } else {
        None
    };
    render::render_event_detail(
        &state,
        &viewer,
        &event,
        &schedule,
        &cap,
        admin_extras.as_ref(),
    )
}

async fn event_json(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    let viewer = viewer(&state, session.as_ref(), &headers);
    let event = state
        .events
        .get_event(&event_ref, &viewer)
        .await
        .map_err(event_error)?;
    let schedule = state
        .events
        .store()
        .schedule_for_event(&event.id)
        .await
        .map_err(EventError::from)
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({
        "viewer": viewer,
        "event": event,
        "schedule": schedule,
    })))
}

async fn create_event(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Json(input): Json<CreateEvent>,
) -> Result<Json<serde_json::Value>, Response> {
    let v = require_admin(&state, session.as_ref(), &headers)?;
    let mut input = input;
    input.created_by_isoastra_identity_id = v.admin_identity_id().map(ToOwned::to_owned);
    let event = state
        .events
        .create_event(input)
        .await
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({ "event": event })))
}

async fn update_event(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
    Json(input): Json<UpdateEvent>,
) -> Result<Json<serde_json::Value>, Response> {
    let v = require_admin(&state, session.as_ref(), &headers)?;
    let event = state
        .events
        .update_event(&event_ref, input, v.admin_identity_id())
        .await
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({ "event": event })))
}

async fn publish_event(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    let v = require_admin(&state, session.as_ref(), &headers)?;
    let event = state
        .events
        .set_event_status(&event_ref, EventStatus::Published, v.admin_identity_id())
        .await
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({ "event": event })))
}

async fn archive_event(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    let v = require_admin(&state, session.as_ref(), &headers)?;
    let event = state
        .events
        .set_event_status(&event_ref, EventStatus::Archived, v.admin_identity_id())
        .await
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({ "event": event })))
}

#[derive(Debug, Deserialize)]
struct ScheduleRequest {
    title: String,
    sort_order: Option<i64>,
}

async fn create_schedule_item(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
    Json(input): Json<ScheduleRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    let admin = require_admin(&state, session.as_ref(), &headers)?;
    let event = state
        .events
        .get_event(&event_ref, &admin)
        .await
        .map_err(event_error)?;
    let item = state
        .events
        .create_schedule_item(&event.id, input.title, input.sort_order.unwrap_or(0))
        .await
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({ "schedule_item": item })))
}

async fn create_invitee(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
    Json(mut input): Json<CreateInvitee>,
) -> Result<Json<serde_json::Value>, Response> {
    let admin = require_admin(&state, session.as_ref(), &headers)?;
    let event = state
        .events
        .get_event(&event_ref, &admin)
        .await
        .map_err(event_error)?;
    input.event_id = event.id;
    let created = state
        .events
        .create_invitee(input)
        .await
        .map_err(event_error)?;
    Ok(Json(
        serde_json::json!({ "invitee": created.invitee, "rsvp_url": created.rsvp_url, "token": created.raw_token }),
    ))
}

async fn list_invitees_json(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    let admin = require_admin(&state, session.as_ref(), &headers)?;
    let event = state
        .events
        .get_event(&event_ref, &admin)
        .await
        .map_err(event_error)?;
    let invitees = state
        .events
        .store()
        .invitees_for_event(&event.id)
        .await
        .map_err(EventError::from)
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({ "invitees": invitees })))
}

async fn approve_invitee(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path((event_ref, invitee_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, Response> {
    let admin = require_admin(&state, session.as_ref(), &headers)?;
    let invitee = state
        .events
        .approve_invitee_location(&event_ref, &invitee_id, admin.admin_identity_id())
        .await
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({ "invitee": invitee })))
}

async fn regenerate_invitee_token(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path((event_ref, invitee_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, Response> {
    let admin = require_admin(&state, session.as_ref(), &headers)?;
    let event = state
        .events
        .get_event(&event_ref, &admin)
        .await
        .map_err(event_error)?;
    let invitee = state
        .events
        .store()
        .invitee_by_id(&invitee_id)
        .await
        .map_err(EventError::from)
        .map_err(event_error)?
        .ok_or_else(|| event_error(EventError::NotFound))?;
    if invitee.event_id != event.id {
        return Err(event_error(EventError::NotFound));
    }
    let (raw_token, rsvp_url) = state
        .events
        .rotate_invitee_token(&invitee_id)
        .await
        .map_err(event_error)?;
    Ok(Json(
        serde_json::json!({ "token": raw_token, "rsvp_url": rsvp_url }),
    ))
}

async fn create_script(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
    Json(mut input): Json<CreateScript>,
) -> Result<Json<serde_json::Value>, Response> {
    let admin = require_admin(&state, session.as_ref(), &headers)?;
    let event = state
        .events
        .get_event(&event_ref, &admin)
        .await
        .map_err(event_error)?;
    input.event_id = event.id;
    let script = state
        .events
        .create_script(input)
        .await
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({ "script": script })))
}

#[derive(Debug, Deserialize)]
struct RenderScriptRequest {
    invitee_id: String,
    raw_token: String,
}

async fn render_script_json(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path((event_ref, script_id)): Path<(String, String)>,
    Json(input): Json<RenderScriptRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    let admin = require_admin(&state, session.as_ref(), &headers)?;
    let event = state
        .events
        .get_event(&event_ref, &admin)
        .await
        .map_err(event_error)?;
    let rendered = state
        .events
        .render_script_for_invitee(&script_id, &input.invitee_id, &input.raw_token)
        .await
        .map_err(event_error)?;
    let _ = state
        .events
        .log_copy(
            &event.id,
            Some(&input.invitee_id),
            Some(&script_id),
            admin.admin_identity_id(),
            Some(&rendered.rendered),
        )
        .await
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({
        "rendered": rendered.rendered,
        "rendered_hash": rendered.rendered_hash,
    })))
}

async fn create_signup_token(
    State(state): State<AppState>,
    session: Option<Extension<SessionData>>,
    headers: HeaderMap,
    Path(event_ref): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    let _ = require_admin(&state, session.as_ref(), &headers)?;
    let (raw, event) = state
        .events
        .create_signup_token(&event_ref)
        .await
        .map_err(event_error)?;
    let ref_ = event.slug.as_deref().unwrap_or(&event.id);
    let signup_url = format!(
        "{}/events/{}/signup?t={}",
        state.public_base_url.trim_end_matches('/'),
        urlencoding::encode(ref_),
        urlencoding::encode(&raw)
    );
    Ok(Json(
        serde_json::json!({ "token": raw, "signup_url": signup_url }),
    ))
}

async fn invitee_by_token(
    State(state): State<AppState>,
    Path((event_ref, token)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, Response> {
    let invitee = state
        .events
        .resolve_invitee_token(&event_ref, &token)
        .await
        .map_err(event_error)?;
    let guests = state
        .events
        .store()
        .guests_for_invitee(&invitee.id)
        .await
        .map_err(EventError::from)
        .map_err(event_error)?;
    Ok(Json(serde_json::json!({
        "viewer": Viewer::Invitee { invitee_id: invitee.id.clone(), event_id: invitee.event_id.clone() },
        "invitee": invitee,
        "guests": guests,
    })))
}

async fn rsvp_html(
    State(state): State<AppState>,
    Path((event_ref, token)): Path<(String, String)>,
) -> Response {
    let invitee = match state.events.resolve_invitee_token(&event_ref, &token).await {
        Ok(i) => i,
        Err(err) => return event_error(err),
    };
    let event = match state
        .events
        .get_event(
            &invitee.event_id,
            &Viewer::Invitee {
                invitee_id: invitee.id.clone(),
                event_id: invitee.event_id.clone(),
            },
        )
        .await
    {
        Ok(e) => e,
        Err(err) => return event_error(err),
    };
    let guests = match state.events.store().guests_for_invitee(&invitee.id).await {
        Ok(g) => g,
        Err(err) => return event_error(EventError::from(err)),
    };
    render::render_rsvp(&state, &event, &invitee, &guests, &token)
}

async fn update_rsvp(
    State(state): State<AppState>,
    Path((event_ref, token)): Path<(String, String)>,
    Json(input): Json<RsvpUpdate>,
) -> Result<Json<serde_json::Value>, Response> {
    let invitee = state
        .events
        .update_rsvp_by_token(&event_ref, &token, input)
        .await
        .map_err(event_error)?;
    let guests = state
        .events
        .store()
        .guests_for_invitee(&invitee.id)
        .await
        .map_err(EventError::from)
        .map_err(event_error)?;
    Ok(Json(
        serde_json::json!({ "invitee": invitee, "guests": guests }),
    ))
}

#[derive(Debug, Deserialize)]
struct SignupQuery {
    t: Option<String>,
}

async fn signup_html(
    State(state): State<AppState>,
    Path(event_ref): Path<String>,
    Query(query): Query<SignupQuery>,
) -> Response {
    let (event, signed_signup) = match state
        .events
        .get_signup_event(&event_ref, query.t.as_deref())
        .await
    {
        Ok(v) => v,
        Err(err) => return event_error(err),
    };
    let cap = match capacity::display(state.events.store(), &event).await {
        Ok(c) => c,
        Err(err) => return event_error(err),
    };
    render::render_signup(&state, &event, &cap, query.t.as_deref(), signed_signup)
}

async fn self_signup(
    State(state): State<AppState>,
    Path(event_ref): Path<String>,
    Query(query): Query<SignupQuery>,
    Json(input): Json<SelfSignupInput>,
) -> Result<Json<serde_json::Value>, Response> {
    let created = state
        .events
        .self_signup(&event_ref, query.t.as_deref(), input)
        .await
        .map_err(event_error)?;
    Ok(Json(
        serde_json::json!({ "invitee": created.invitee, "rsvp_url": created.rsvp_url, "token": created.raw_token }),
    ))
}

async fn event_capacity_json(
    State(state): State<AppState>,
    Path(event_ref): Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    let event = state
        .events
        .get_event(&event_ref, &Viewer::Anonymous)
        .await
        .map_err(event_error)?;
    let cap = capacity::display(state.events.store(), &event)
        .await
        .map_err(event_error)?;
    // Scrub admin-only fields for public JSON.
    Ok(Json(serde_json::json!({ "capacity": {
        "public_confirmed": cap.public_confirmed,
        "cap": cap.cap,
        "self_signup_open": cap.self_signup_open,
    }})))
}

async fn signup_qr(
    State(state): State<AppState>,
    Path(event_ref): Path<String>,
) -> Result<Response, Response> {
    let event = state
        .events
        .get_event(&event_ref, &Viewer::Anonymous)
        .await
        .map_err(event_error)?;
    let event_ref = event.slug.as_deref().unwrap_or(&event.id);
    let url = format!(
        "{}/events/{}/signup",
        state.public_base_url.trim_end_matches('/'),
        urlencoding::encode(event_ref)
    );
    let svg = qr::svg_for_url(&url).map_err(event_error)?;
    Ok(([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response())
}

async fn event_ics(
    State(state): State<AppState>,
    Path(event_ref): Path<String>,
) -> Result<Response, Response> {
    let event = state
        .events
        .get_event(&event_ref, &Viewer::Anonymous)
        .await
        .map_err(event_error)?;
    let body = calendar_body(&event);
    Ok(([(header::CONTENT_TYPE, "text/calendar")], body).into_response())
}

fn viewer(
    state: &AppState,
    session: Option<&Extension<SessionData>>,
    headers: &HeaderMap,
) -> Viewer {
    if state.dev_mode && has_cookie(headers, DEV_ADMIN_COOKIE, "1") {
        return Viewer::Admin {
            isoastra_identity_id: "dev-admin".to_owned(),
            role: Some("owner".to_owned()),
        };
    }
    viewer_from_session(&state.admins, session.map(|Extension(data)| data))
}

fn require_admin(
    state: &AppState,
    session: Option<&Extension<SessionData>>,
    headers: &HeaderMap,
) -> Result<Viewer, Response> {
    let viewer = viewer(state, session, headers);
    if viewer.is_admin() {
        Ok(viewer)
    } else {
        Err((StatusCode::NOT_FOUND, "not found").into_response())
    }
}

fn has_cookie(headers: &HeaderMap, name: &str, value: &str) -> bool {
    let Some(raw) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    raw.split(';').any(|kv| {
        let kv = kv.trim();
        let Some((k, v)) = kv.split_once('=') else {
            return false;
        };
        k == name && v == value
    })
}

#[allow(clippy::needless_pass_by_value)]
fn event_error(err: EventError) -> Response {
    tracing::warn!(error = %err, "event backend error");
    (err.status_code(), err.public_message()).into_response()
}

fn calendar_body(event: &Event) -> String {
    let uid = format!("{}@ronitnath.com", event.id);
    let location = event.approximate_location_name.clone().unwrap_or_default();
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//ronitnath//events//EN\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nSUMMARY:{summary}\r\nDTSTART:{start}\r\nDTEND:{end}\r\nLOCATION:{location}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        summary = escape_ics(&event.title),
        start = escape_ics(&event.starts_at),
        end = escape_ics(&event.ends_at),
        location = escape_ics(&location),
    )
}

fn escape_ics(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
        .replace('\n', "\\n")
}
