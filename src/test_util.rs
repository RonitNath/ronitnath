//! Shared helpers for HTTP-level ("router") tests — see AGENTS.md → "Router
//! tests". Exercises [`crate::app::build_router`] directly through
//! `tower::ServiceExt::oneshot`: no listener, no port, so it stays inside
//! the existing "<~1s, parallel-safe" test budget (see AGENTS.md).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::connect_info::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode, header};
use serde::Serialize;
use tower::ServiceExt;

use crate::app::build_router;
use crate::config::Config;
use crate::state::AppState;
use crate::store::Store;

/// A fresh router over its own in-memory database and rate limiter, built
/// with [`Config::for_tests`]. Every call is fully isolated, so tests using
/// it can run in parallel (the crate-wide requirement — see AGENTS.md).
pub async fn test_app() -> Router {
    let store = Store::connect_in_memory().await;
    let state = AppState::new(store);
    build_router(state, &Config::for_tests())
}

/// Source address for tests that don't care about rate limiting; use
/// [`post_json_from`] with a distinct address for tests that do.
const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));

pub async fn get(app: &Router, path: &str) -> (StatusCode, HeaderMap, Bytes) {
    send(app, Request::get(path).body(Body::empty()).unwrap(), DEFAULT_IP).await
}

pub async fn post_json<T: Serialize>(app: &Router, path: &str, body: &T) -> (StatusCode, HeaderMap, Bytes) {
    post_json_from(app, path, body, DEFAULT_IP).await
}

pub async fn post_json_from<T: Serialize>(
    app: &Router,
    path: &str,
    body: &T,
    ip: IpAddr,
) -> (StatusCode, HeaderMap, Bytes) {
    post_bytes_from(app, path, "application/json", serde_json::to_vec(body).unwrap(), ip).await
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

/// Sends `request` through `app` via `oneshot`, first stamping a
/// `ConnectInfo` extension — normally added by
/// `into_make_service_with_connect_info` (see `app::run`), which only
/// exists once there's a real listener. Rate-limiting middleware reads
/// this, so every test helper goes through here rather than calling
/// `oneshot` directly.
async fn send(app: &Router, mut request: Request<Body>, ip: IpAddr) -> (StatusCode, HeaderMap, Bytes) {
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
