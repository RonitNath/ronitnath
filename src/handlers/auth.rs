//! Signup, login, logout pages and their form handlers.

use askama::Template;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Extension, Form, http::HeaderMap};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use std::net::SocketAddr;

use crate::auth::extract::{NavContext, NavUser};
use crate::auth::login::RequestContext;
use crate::auth::{AccountScope, csrf, login, oidc, session};
use crate::error::AppError;
use crate::state::AppState;
use crate::store::sessions::SessionContext;
use crate::view::render;

#[derive(Deserialize)]
pub struct NextQuery {
    next: Option<String>,
}

/// Only accepts an in-app path (`/...`), never a `//host` or absolute URL
/// — otherwise `next` becomes an open-redirect gadget.
fn safe_next(next: Option<String>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") => n,
        _ => "/".to_string(),
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

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[derive(Deserialize)]
pub struct OidcStartQuery {
    intent: Option<String>,
    csrf_token: Option<String>,
    next: Option<String>,
}

pub async fn oidc_start(
    State(state): State<AppState>,
    Extension(session_ctx): Extension<Option<SessionContext>>,
    Path(key): Path<String>,
    Query(query): Query<OidcStartQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let intent = match query.intent.as_deref() {
        Some("link") => oidc::OidcIntent::Link,
        _ => oidc::OidcIntent::Login,
    };
    let (identity_id, account_id) = if intent == oidc::OidcIntent::Link {
        let ctx =
            session_ctx.ok_or_else(|| AppError::Forbidden("sign in before linking SSO".into()))?;
        match query.csrf_token.as_deref() {
            Some(token) if constant_time_eq(token, &ctx.csrf_token) => {
                (Some(ctx.identity_id), Some(ctx.account_id))
            }
            _ => return Err(AppError::Forbidden("missing or invalid CSRF token".into())),
        }
    } else {
        (None, None)
    };
    let redirect_uri = oidc::redirect_uri(&headers, &key)?;
    let auth_url = oidc::start(
        &state,
        &key,
        redirect_uri,
        intent,
        identity_id,
        account_id,
        query.next.map(|n| safe_next(Some(n))),
        None,
    )
    .await?;
    Ok(Redirect::to(&auth_url).into_response())
}

#[derive(Deserialize)]
pub struct OidcCallbackQuery {
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
    Query(query): Query<OidcCallbackQuery>,
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
    let (outcome, next) = oidc::callback(
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
    let jar = CookieJar::new().add(cookie);
    Ok((jar, Redirect::to(&safe_next(next))).into_response())
}

#[derive(Template)]
#[template(path = "auth/login.html")]
struct LoginTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    next: String,
    oidc_providers: Vec<oidc::OidcProviderButton>,
}

pub async fn login_page(
    State(state): State<AppState>,
    NavContext(current_user): NavContext,
    Query(query): Query<NextQuery>,
) -> Result<Response, AppError> {
    if current_user.is_some() {
        return Ok(Redirect::to("/").into_response());
    }
    render(LoginTemplate {
        nav_active: "",
        current_user: None,
        next: safe_next(query.next),
        oidc_providers: state.oidc().buttons(),
    })
}

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
    next: Option<String>,
}

pub async fn login_submit(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Result<Response, AppError> {
    let ip = addr.ip().to_string();
    let outcome = login::login(
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
    let jar = CookieJar::new().add(cookie);
    Ok((jar, Redirect::to(&safe_next(form.next))).into_response())
}

#[derive(Template)]
#[template(path = "auth/signup.html")]
struct SignupTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    signup_open: bool,
}

pub async fn signup_page(
    State(state): State<AppState>,
    NavContext(current_user): NavContext,
) -> Result<Response, AppError> {
    if current_user.is_some() {
        return Ok(Redirect::to("/").into_response());
    }
    render(SignupTemplate {
        nav_active: "",
        current_user: None,
        signup_open: state.auth_config().signup_open,
    })
}

#[derive(Deserialize)]
pub struct SignupForm {
    display_name: String,
    email: String,
    password: String,
}

pub async fn signup_submit(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<SignupForm>,
) -> Result<Response, AppError> {
    if !state.auth_config().signup_open {
        return Err(AppError::Forbidden(
            "signup is closed on this deployment".into(),
        ));
    }
    let ip = addr.ip().to_string();
    let outcome = login::signup(
        &state,
        &form.display_name,
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
    let jar = CookieJar::new().add(cookie);
    Ok((jar, Redirect::to("/")).into_response())
}

#[derive(Deserialize)]
pub struct LogoutForm {
    csrf_token: String,
}

pub async fn logout_submit(
    State(state): State<AppState>,
    scope: AccountScope,
    Form(form): Form<LogoutForm>,
) -> Result<Response, AppError> {
    csrf::verify(&scope, &form.csrf_token)?;
    let session_id = scope.session_id.ok_or_else(|| {
        AppError::Forbidden("bearer-token auth has no session to log out of".into())
    })?;
    login::logout(&state, session_id, scope.identity_id).await?;

    let cookie = session::clear_cookie(state.auth_config().cookie_secure);
    let jar = CookieJar::new().add(cookie);
    Ok((jar, Redirect::to("/")).into_response())
}
