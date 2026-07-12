//! Shared helpers for HTTP-level ("router") tests — see AGENTS.md → "Router
//! tests". Exercises the split routers directly through
//! `tower::ServiceExt::oneshot`: no listener, no port, so it stays inside
//! the existing "<~1s, parallel-safe" test budget (see AGENTS.md).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::connect_info::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode, header};
use serde::Serialize;
use tower::ServiceExt;

use crate::app::{build_admin_router, build_site_router, test_auth_config};
use crate::auth::session;
use crate::config::Config;
use crate::state::AppState;
use crate::store::Store;

/// A fresh router over its own in-memory database and rate limiter, built
/// with [`Config::for_tests`]. Every call is fully isolated, so tests using
/// it can run in parallel (the crate-wide requirement — see AGENTS.md).
/// Returns the underlying [`Store`] too — tests that need to seed data
/// (a second membership, a revoked session) too awkward to reach through
/// HTTP alone can use it directly; it's the same store the router queries.
pub async fn test_app() -> (Router, Store) {
    let store = Store::connect_in_memory().await;
    let config = Config::for_tests();
    let state = AppState::new(store.clone(), test_auth_config(&config));
    (build_admin_router(state, &config), store)
}

/// A fresh public router with no session/auth middleware or auth routes.
pub async fn test_site_app() -> (Router, Store) {
    let store = Store::connect_in_memory().await;
    let config = Config::for_tests();
    let state = AppState::new(store.clone(), test_auth_config(&config));
    (build_site_router(state, &config), store)
}

/// Source address for tests that don't care about rate limiting; use
/// [`post_json_from`] with a distinct address for tests that do.
const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));

pub async fn get(app: &Router, path: &str) -> (StatusCode, HeaderMap, Bytes) {
    send(
        app,
        Request::get(path).body(Body::empty()).unwrap(),
        DEFAULT_IP,
    )
    .await
}

pub async fn get_with_cookie(
    app: &Router,
    path: &str,
    cookie: &str,
) -> (StatusCode, HeaderMap, Bytes) {
    let request = Request::get(path)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    send(app, request, DEFAULT_IP).await
}

pub async fn post_json_from<T: Serialize>(
    app: &Router,
    path: &str,
    body: &T,
    ip: IpAddr,
) -> (StatusCode, HeaderMap, Bytes) {
    post_bytes_from(
        app,
        path,
        "application/json",
        serde_json::to_vec(body).unwrap(),
        ip,
    )
    .await
}

/// A JSON POST carrying a session cookie and (optionally) a CSRF header —
/// what an authenticated JSON API mutation looks like.
pub async fn post_json_authed<T: Serialize>(
    app: &Router,
    path: &str,
    body: &T,
    cookie: &str,
    csrf_token: Option<&str>,
) -> (StatusCode, HeaderMap, Bytes) {
    let mut builder = Request::post(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, cookie);
    if let Some(token) = csrf_token {
        builder = builder.header("x-csrf-token", token);
    }
    send(
        app,
        builder
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap(),
        DEFAULT_IP,
    )
    .await
}

pub async fn post_bytes(
    app: &Router,
    path: &str,
    content_type: &str,
    body: Vec<u8>,
) -> (StatusCode, HeaderMap, Bytes) {
    post_bytes_from(app, path, content_type, body, DEFAULT_IP).await
}

async fn post_bytes_from(
    app: &Router,
    path: &str,
    content_type: &str,
    body: Vec<u8>,
    ip: IpAddr,
) -> (StatusCode, HeaderMap, Bytes) {
    let request = Request::post(path)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .unwrap();
    send(app, request, ip).await
}

/// A `application/x-www-form-urlencoded` POST — what plain HTML forms
/// (login/signup/logout/settings) submit. `form` is a pre-encoded query
/// string, e.g. `"email=a%40b.com&password=hunter2"`.
pub async fn post_form(app: &Router, path: &str, form: &str) -> (StatusCode, HeaderMap, Bytes) {
    post_form_with_cookie(app, path, form, None).await
}

pub async fn post_form_with_cookie(
    app: &Router,
    path: &str,
    form: &str,
    cookie: Option<&str>,
) -> (StatusCode, HeaderMap, Bytes) {
    let mut builder =
        Request::post(path).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    send(
        app,
        builder.body(Body::from(form.to_string())).unwrap(),
        DEFAULT_IP,
    )
    .await
}

/// Pulls a `name=value` pair (dropping cookie attributes) out of a
/// response's `Set-Cookie` headers — panics if `name` wasn't set, since
/// every caller uses this right after an action that's supposed to set it.
pub fn extract_cookie(headers: &HeaderMap, name: &str) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|v| {
            let s = v.to_str().ok()?;
            let prefix = format!("{name}=");
            s.starts_with(&prefix)
                .then(|| s.split(';').next().unwrap().to_string())
        })
        .unwrap_or_else(|| panic!("no Set-Cookie for {name} in {headers:?}"))
}

/// The csrf token embedded in a rendered page's `<meta name="csrf-token">`
/// tag — the same value `ts/src/lib/api.ts` reads client-side.
pub fn extract_csrf_token(body: &Bytes) -> String {
    let body = std::str::from_utf8(body).unwrap();
    let marker = r#"<meta name="csrf-token" content=""#;
    let start = body.find(marker).expect("no csrf-token meta tag in page") + marker.len();
    let end = start + body[start..].find('"').unwrap();
    body[start..end].to_string()
}

pub struct Authed {
    pub cookie: String,
    pub csrf_token: String,
}

/// Signs up a fresh identity (its own auto-created personal account, role
/// `owner`) through the real HTTP form flow and returns its session
/// cookie + CSRF token, ready for authenticated requests.
pub async fn signup(app: &Router, display_name: &str, email: &str, password: &str) -> Authed {
    let form = format!(
        "display_name={}&email={}&password={}",
        urlencoding(display_name),
        urlencoding(email),
        urlencoding(password)
    );
    let (status, headers, _) = post_form(app, "/signup", &form).await;
    assert_eq!(
        status,
        StatusCode::SEE_OTHER,
        "signup should redirect on success"
    );
    let cookie = extract_cookie(&headers, "session");

    let (status, _, body) = get_with_cookie(app, "/settings", &cookie).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "signed-up identity should be able to load /settings"
    );
    let csrf_token = extract_csrf_token(&body);

    Authed { cookie, csrf_token }
}

/// Creates a session directly (bypassing login) for `identity_id` with
/// `account_id` active — for scenarios HTTP can't reach yet in phase 1
/// (a second identity holding a non-owner membership on someone else's
/// account, since there's no invite flow to produce one through the UI).
/// Returns a ready-to-use `Authed` just like [`signup`].
pub async fn seed_session(store: &Store, identity_id: i64, account_id: i64) -> Authed {
    let raw_token = session::generate_token();
    let token_hash = session::hash_token(&raw_token);
    let csrf_token = session::generate_token();
    store
        .create_session(
            identity_id,
            account_id,
            &token_hash,
            &csrf_token,
            "9999-01-01 00:00:00",
            None,
            None,
        )
        .await
        .unwrap();
    Authed {
        cookie: format!("session={raw_token}"),
        csrf_token,
    }
}

/// Percent-encodes the handful of characters test fixtures actually use
/// (`@`, spaces) — not a general-purpose encoder, just enough for test
/// emails/passwords/names.
fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '@' => "%40".to_string(),
            ' ' => "+".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// Sends `request` through `app` via `oneshot`, first stamping a
/// `ConnectInfo` extension — normally added by
/// `into_make_service_with_connect_info` (see `app::run`), which only
/// exists once there's a real listener. Rate-limiting middleware reads
/// this, so every test helper goes through here rather than calling
/// `oneshot` directly.
async fn send(
    app: &Router,
    mut request: Request<Body>,
    ip: IpAddr,
) -> (StatusCode, HeaderMap, Bytes) {
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(ip, 0)));
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, body)
}
