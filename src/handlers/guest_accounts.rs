//! Guest claim, recovery-email login, and session-scoped event browsing.

use std::net::SocketAddr;

use askama::Template;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Extension, Form, Json};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use time::macros::format_description;

use crate::access::level::Level;
use crate::auth::extract::{GuestScope, NavContext, NavUser};
use crate::auth::login::{self, RequestContext};
use crate::auth::session;
use crate::auth::viewer::Viewer;
use crate::auth::{csrf, oidc, password};
use crate::dates as filters;
use crate::error::AppError;
use crate::handlers::event_public::{GuestView, RsvpResult, RsvpSubmit};
use crate::state::AppState;
use crate::store::event_links::ResolvedLink;
use crate::store::events::{Event, EventView};
use crate::store::sessions::SessionContext;
use crate::view::render;

fn claim_conflict(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::RowNotFound => true,
        sqlx::Error::Database(db) => {
            db.is_unique_violation()
                || db
                    .code()
                    .and_then(|code| code.parse::<i32>().ok())
                    .is_some_and(|code| matches!(code & 0xff, 5 | 6))
        }
        _ => false,
    }
}

fn request_context<'a>(headers: &'a HeaderMap, ip: &'a str) -> RequestContext<'a> {
    RequestContext {
        request_id: headers.get("x-request-id").and_then(|v| v.to_str().ok()),
        ip: Some(ip),
        user_agent: headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok()),
    }
}

#[derive(Template)]
#[template(path = "guest/claim.html")]
struct ClaimTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    person_name: String,
    csrf_token: String,
    claim_token: String,
    oidc_providers: Vec<oidc::OidcProviderButton>,
}

pub async fn claim_page(
    State(state): State<AppState>,
    NavContext(current_user): NavContext,
    Path(raw): Path<String>,
) -> Result<Response, AppError> {
    let (link, _) = crate::handlers::event_public::resolve(state.store(), &raw).await?;
    let person_id = link.person_id.ok_or(AppError::NotFound)?;
    if state
        .store()
        .active_identity_for_person(link.account_id, person_id)
        .await?
        .is_some()
    {
        return Err(AppError::NotFound); // claimed and invalid capabilities share the same surface
    }
    let person = state
        .store()
        .find_person(link.account_id, person_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let csrf_token = current_user
        .as_ref()
        .map(|u| u.csrf_token.clone())
        .unwrap_or_default();
    render(ClaimTemplate {
        nav_active: "",
        current_user,
        person_name: person.name,
        csrf_token,
        claim_token: raw,
        oidc_providers: state.oidc().buttons(),
    })
}

#[derive(Deserialize)]
pub struct ClaimForm {
    password: String,
    password_confirm: String,
    recovery_email: String,
    csrf_token: String,
}

pub async fn claim_submit(
    State(state): State<AppState>,
    Extension(session_ctx): Extension<Option<SessionContext>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(raw): Path<String>,
    Form(form): Form<ClaimForm>,
) -> Result<Response, AppError> {
    csrf::verify_optional(
        session_ctx.as_ref().map(|s| s.csrf_token.as_str()),
        &form.csrf_token,
    )?;
    let (link, _) = crate::handlers::event_public::resolve(state.store(), &raw).await?;
    let person_id = link.person_id.ok_or(AppError::NotFound)?;
    if state
        .store()
        .active_identity_for_person(link.account_id, person_id)
        .await?
        .is_some()
    {
        return Err(AppError::NotFound);
    }
    if form.password.len() < 8 || form.password != form.password_confirm {
        return Err(AppError::Invalid(
            "matching passwords of at least 8 characters are required".into(),
        ));
    }
    let person = state
        .store()
        .find_person(link.account_id, person_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let recovery_email = form.recovery_email.trim().to_lowercase();
    let recovery_email = (!recovery_email.is_empty()).then_some(recovery_email);
    if recovery_email
        .as_ref()
        .is_some_and(|email| !email.contains('@'))
    {
        return Err(AppError::Invalid(
            "enter a valid recovery email or leave it blank".into(),
        ));
    }
    let password_hash =
        password::hash(&form.password).map_err(|e| anyhow::anyhow!("hashing password: {e}"))?;
    let raw_session = session::generate_token();
    let session_hash = session::hash_token(&raw_session);
    let session_csrf = session::generate_token();
    let ttl_secs = state.auth_config().session_ttl_secs;
    let expires_at = (time::OffsetDateTime::now_utc() + time::Duration::seconds(ttl_secs))
        .format(&format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .map_err(|e| anyhow::anyhow!("formatting session expiry: {e}"))?;
    let ip = addr.ip().to_string();
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let (identity_id, guest_account_id) = state
        .store()
        .claim_guest(
            link.account_id,
            person_id,
            &person.name,
            recovery_email.as_deref(),
            &password_hash,
            &session_hash,
            &session_csrf,
            &expires_at,
            user_agent,
            Some(&ip),
        )
        .await
        .map_err(|error| {
            if claim_conflict(&error) {
                AppError::NotFound
            } else {
                AppError::from(error)
            }
        })?;
    state
        .store()
        .audit(
            Some(identity_id),
            Some(link.account_id),
            headers.get("x-request-id").and_then(|v| v.to_str().ok()),
            "guest.claimed",
            "person",
            Some(&person_id.to_string()),
            &serde_json::json!({"guest_account_id": guest_account_id, "event_link_id": link.id}),
        )
        .await?;
    let cookie = session::build_cookie(state.auth_config().cookie_secure, raw_session, ttl_secs);
    Ok((CookieJar::new().add(cookie), Redirect::to("/my")).into_response())
}

#[derive(Template)]
#[template(path = "guest/login.html")]
struct GuestLoginTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    oidc_providers: Vec<oidc::OidcProviderButton>,
}

pub async fn login_page(
    State(state): State<AppState>,
    NavContext(current_user): NavContext,
) -> Result<Response, AppError> {
    if current_user.is_some() {
        return Ok(Redirect::to("/my").into_response());
    }
    render(GuestLoginTemplate {
        nav_active: "",
        current_user: None,
        oidc_providers: state.oidc().buttons(),
    })
}

#[derive(Deserialize)]
pub struct GuestOidcStartQuery {
    claim: Option<String>,
}

pub async fn oidc_start(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<GuestOidcStartQuery>,
) -> Result<Response, AppError> {
    let guest_claim = match query.claim {
        Some(raw) => {
            let (link, _) = crate::handlers::event_public::resolve(state.store(), &raw).await?;
            let person_id = link.person_id.ok_or(AppError::NotFound)?;
            if state
                .store()
                .active_identity_for_person(link.account_id, person_id)
                .await?
                .is_some()
            {
                return Err(AppError::NotFound);
            }
            Some(oidc::GuestOidcClaim {
                event_link_id: link.id,
                owner_account_id: link.account_id,
                person_id,
            })
        }
        None => None,
    };
    let redirect_uri = oidc::canonical_redirect_uri(state.public_url(), &key)?;
    let auth_url = oidc::start(
        &state,
        &key,
        redirect_uri,
        oidc::OidcIntent::GuestLogin,
        None,
        None,
        Some("/my".into()),
        guest_claim,
    )
    .await?;
    Ok((StatusCode::FOUND, [(header::LOCATION, auth_url)]).into_response())
}

#[derive(Deserialize)]
pub struct GuestOidcCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub async fn oidc_callback(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Query(query): Query<GuestOidcCallbackQuery>,
) -> Result<Response, AppError> {
    if let Some(error) = query.error {
        let description = query.error_description.unwrap_or_default();
        return Err(AppError::InvalidCredentials(format!(
            "OIDC provider returned {error}: {description}"
        )));
    }
    let code = query
        .code
        .ok_or_else(|| AppError::InvalidCredentials("OIDC callback missing code".into()))?;
    let csrf_state = query
        .state
        .ok_or_else(|| AppError::InvalidCredentials("OIDC callback missing state".into()))?;
    let ip = addr.ip().to_string();
    let (outcome, _) = oidc::callback(
        &state,
        &key,
        code,
        csrf_state,
        request_context(&headers, &ip),
    )
    .await?;
    let cookie = session::build_cookie(
        state.auth_config().cookie_secure,
        outcome.raw_token,
        outcome.ttl_secs,
    );
    Ok((CookieJar::new().add(cookie), Redirect::to("/my")).into_response())
}

#[derive(Deserialize)]
pub struct GuestLoginForm {
    email: String,
    password: String,
}

pub async fn login_submit(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<GuestLoginForm>,
) -> Result<Response, AppError> {
    let ip = addr.ip().to_string();
    let outcome = login::guest_login(
        &state,
        &form.email,
        &form.password,
        request_context(&headers, &ip),
    )
    .await?;
    let cookie = session::build_cookie(
        state.auth_config().cookie_secure,
        outcome.raw_token,
        outcome.ttl_secs,
    );
    Ok((CookieJar::new().add(cookie), Redirect::to("/my")).into_response())
}

#[derive(Deserialize)]
pub struct LogoutForm {
    csrf_token: String,
}

pub async fn logout_submit(
    State(state): State<AppState>,
    Extension(session_ctx): Extension<Option<SessionContext>>,
    Form(form): Form<LogoutForm>,
) -> Result<Response, AppError> {
    if let Some(session_ctx) = session_ctx {
        csrf::verify_optional(Some(&session_ctx.csrf_token), &form.csrf_token)?;
        login::logout(&state, session_ctx.session_id, session_ctx.identity_id).await?;
    }
    Ok((
        CookieJar::new().add(session::clear_cookie(state.auth_config().cookie_secure)),
        Redirect::to("/"),
    )
        .into_response())
}

struct DashboardEvent {
    id: i64,
    title: String,
    starts_at: String,
    rsvp_status: Option<String>,
}
#[derive(Template)]
#[template(path = "guest/my.html")]
struct MyTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    events: Vec<DashboardEvent>,
}

fn nav(scope: &GuestScope) -> NavUser {
    NavUser {
        display_name: scope.person_name.clone(),
        csrf_token: scope.csrf_token.clone(),
        is_guest: true,
    }
}

pub async fn my_page(
    State(state): State<AppState>,
    scope: GuestScope,
) -> Result<Response, AppError> {
    let viewer = Viewer::Guest {
        identity_id: scope.identity_id,
        person_id: scope.person_id,
    };
    let now = time::OffsetDateTime::now_utc()
        .format(&format_description!("[year]-[month]-[day] [hour]:[minute]"))
        .map_err(|e| anyhow::anyhow!("formatting current time: {e}"))?;
    let mut visible = Vec::new();
    for event in state.store().list_events(scope.owner_account_id).await? {
        if event.status == "draft" || event.starts_at < now {
            continue;
        }
        let inputs = state
            .store()
            .audience_inputs_for_event(scope.owner_account_id, event.id, Some(scope.person_id))
            .await?
            .ok_or(AppError::NotFound)?;
        if inputs.level_for(&viewer)? >= Level::Summary {
            let rsvp_status = state
                .store()
                .find_attendance(scope.owner_account_id, event.id, scope.person_id)
                .await?
                .map(|attendance| attendance.status);
            visible.push(DashboardEvent {
                id: event.id,
                title: event.title,
                starts_at: event.starts_at,
                rsvp_status,
            });
        }
    }
    render(MyTemplate {
        nav_active: "my",
        current_user: Some(nav(&scope)),
        events: visible,
    })
}

async fn my_event_parts(
    state: &AppState,
    scope: &GuestScope,
    event_id: i64,
) -> Result<(ResolvedLink, Event, Level), AppError> {
    let event = state
        .store()
        .find_event(scope.owner_account_id, event_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if event.status == "draft" {
        return Err(AppError::NotFound);
    }
    let viewer = Viewer::Guest {
        identity_id: scope.identity_id,
        person_id: scope.person_id,
    };
    let inputs = state
        .store()
        .audience_inputs_for_event(scope.owner_account_id, event_id, Some(scope.person_id))
        .await?
        .ok_or(AppError::NotFound)?;
    let live = state
        .store()
        .find_personal_link(scope.owner_account_id, event_id, scope.person_id)
        .await?;
    let level = match &live {
        Some(link) => inputs.level_for_direct_hit(&viewer, &link.tier)?,
        None => inputs.level_for(&viewer)?,
    };
    // Session browsing has no capability floor when the guest no longer
    // holds a live link. Busy is intentionally list/detail-invisible.
    if level < Level::Summary {
        return Err(AppError::NotFound);
    }
    let link = ResolvedLink {
        id: live.as_ref().map(|l| l.id).unwrap_or(0),
        account_id: scope.owner_account_id,
        event_id,
        person_id: Some(scope.person_id),
        tier: live.map(|l| l.tier).unwrap_or_default(),
    };
    Ok((link, event, level))
}

#[derive(Template)]
#[template(path = "event_public.html")]
struct MyEventTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    rsvp_endpoint: String,
    poster_theme: bool,
    view: GuestView,
    mismatch_note: Option<String>,
    photos: Vec<crate::handlers::photos::GalleryPhoto>,
    photo_upload_url: String,
    photo_csrf: String,
    show_photos: bool,
}

pub async fn my_event_page(
    State(state): State<AppState>,
    scope: GuestScope,
    Path(event_id): Path<i64>,
) -> Result<Response, AppError> {
    let (link, event, level) = my_event_parts(&state, &scope, event_id).await?;
    let view =
        crate::handlers::event_public::build_view(state.store(), &link, &event, level).await?;
    let viewer = Viewer::Guest {
        identity_id: scope.identity_id,
        person_id: scope.person_id,
    };
    let show_photos =
        crate::handlers::photos::attendee(&state, scope.owner_account_id, event_id, &viewer)
            .await?;
    let photo_upload_url = format!("/my/events/{event_id}/photos");
    let photos = if show_photos {
        crate::handlers::photos::gallery(
            &state,
            scope.owner_account_id,
            event_id,
            &viewer,
            &photo_upload_url,
        )
        .await?
    } else {
        Vec::new()
    };
    render(MyEventTemplate {
        nav_active: "my",
        current_user: Some(nav(&scope)),
        rsvp_endpoint: format!("/api/my/events/{event_id}"),
        poster_theme: event.slug == "july4-2026",
        view,
        mismatch_note: None,
        photos,
        photo_upload_url,
        photo_csrf: scope.csrf_token.clone(),
        show_photos,
    })
}

pub async fn api_my_view(
    State(state): State<AppState>,
    scope: GuestScope,
    Path(event_id): Path<i64>,
) -> Result<Json<GuestView>, AppError> {
    let (link, event, level) = my_event_parts(&state, &scope, event_id).await?;
    Ok(Json(
        crate::handlers::event_public::build_view(state.store(), &link, &event, level).await?,
    ))
}

pub async fn api_my_rsvp(
    State(state): State<AppState>,
    scope: GuestScope,
    Path(event_id): Path<i64>,
    headers: HeaderMap,
    Json(submit): Json<RsvpSubmit>,
) -> Result<Json<RsvpResult>, AppError> {
    let submitted_csrf = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    csrf::verify_optional(Some(&scope.csrf_token), submitted_csrf)?;
    let (_link, event, level) = my_event_parts(&state, &scope, event_id).await?;
    if !matches!(submit.status.as_str(), "going" | "maybe" | "no")
        || !(1..=10).contains(&submit.party_size)
        || submit.note.len() > 500
    {
        return Err(AppError::Invalid("invalid RSVP".into()));
    }
    let visible = state
        .store()
        .list_schedule(scope.owner_account_id, event_id, level)
        .await?;
    for choice in &submit.segments {
        if !matches!(choice.status.as_str(), "in" | "maybe" | "out")
            || !visible
                .iter()
                .any(|item| item.id == choice.schedule_item_id && item.segment_key.is_some())
        {
            return Err(AppError::Invalid("unknown schedule segment".into()));
        }
    }
    state
        .store()
        .upsert_attendance(
            scope.owner_account_id,
            event_id,
            scope.person_id,
            &submit.status,
            submit.party_size,
            submit.note.trim(),
        )
        .await?;
    for choice in &submit.segments {
        state
            .store()
            .upsert_segment_rsvp(
                scope.owner_account_id,
                choice.schedule_item_id,
                scope.person_id,
                &choice.status,
            )
            .await?;
    }
    state.store().audit(Some(scope.identity_id), Some(scope.owner_account_id), None, "event.rsvp", "event",
        Some(&event.id.to_string()), &serde_json::json!({"person_id": scope.person_id, "status": submit.status, "via_session": true})).await?;
    Ok(Json(RsvpResult {
        person_name: scope.person_name,
        personal_url: None,
    }))
}
