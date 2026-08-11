//! `/api/whoami`: the narrow session-context contract for user chrome.
//!
//! Pinned here:
//!
//! 1. **The lifecycle.** Anonymous → 401 with a stable JSON shape; signed in →
//!    200 with the DTO; signed out → the same 401 as before sign-in.
//! 2. **The narrowness.** The response must not carry the email address, the
//!    membership role, the session token, or any integer id — the DTO is the
//!    entire contract, and widening it must break a test.
//! 3. **Server-resolved capabilities.** A capability grant shows up in the
//!    list on the next call, because chrome draws nav from it.

mod common;

use axum::http::StatusCode;
use common::{TEST_PASSWORD, TestApp};
use rn_site::auth::{Capability, store};
use serde_json::Value;

const EMAIL: &str = "chrome@example.test";

async fn register_and_signin(app: &TestApp) {
    app.server
        .post("/api/auth/register")
        .form(&[("email", EMAIL), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);
    app.server
        .post("/api/auth/signin")
        .form(&[("email", EMAIL), ("password", TEST_PASSWORD)])
        .await
        .assert_status_ok();
}

#[tokio::test]
async fn whoami_follows_the_session_lifecycle() {
    let app = TestApp::boot().await;

    // Anonymous: stable JSON 401, not an HTML error page.
    let anonymous = app.server.get("/api/whoami").await;
    anonymous.assert_status_unauthorized();
    let body: Value = anonymous.json();
    assert_eq!(body["status"], "anonymous");

    register_and_signin(&app).await;

    let signed_in = app.server.get("/api/whoami").await;
    signed_in.assert_status_ok();
    let body: Value = signed_in.json();

    // The identity/account shape, public ids only.
    assert!(body["identity"]["public_id"].as_str().is_some());
    assert_eq!(body["account"]["kind"], "primary");
    assert_eq!(
        body["account"]["name"], EMAIL,
        "a primary account is named after the normalized address"
    );
    assert!(
        body["expires_at"].as_i64().unwrap() > rn_site::auth::session::now_ms(),
        "a live session expires in the future"
    );

    // A fresh registration holds exactly the owner bundle.
    assert_eq!(
        body["capabilities"],
        serde_json::json!(["test-auth"]),
        "fresh registrant chrome must not see manage"
    );

    // Cache discipline: chrome state must never be cached or shared.
    assert_eq!(
        signed_in.header(axum::http::header::CACHE_CONTROL),
        "no-store"
    );

    // Sign out: back to exactly the anonymous answer.
    app.server
        .post("/api/auth/signout")
        .await
        .assert_status_ok();
    let after = app.server.get("/api/whoami").await;
    after.assert_status_unauthorized();
    let body: Value = after.json();
    assert_eq!(body["status"], "anonymous");

    app.shutdown().await;
}

#[tokio::test]
async fn whoami_is_narrow_and_tracks_grants() {
    let app = TestApp::boot().await;
    register_and_signin(&app).await;

    let body = app.server.get("/api/whoami").await.text();

    // Narrowness: nothing beyond the DTO may appear, starting with the values
    // most tempting to add. The email shows up only as the account *name*
    // (asserted equal above), so probe the fields that must be absent instead.
    let json: Value = serde_json::from_str(&body).expect("whoami is json");
    // serde_json's map is sorted, so compare sorted names.
    let top: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        top,
        ["account", "capabilities", "expires_at", "identity"],
        "the whoami DTO is the entire chrome contract; widening it is an API decision"
    );
    assert!(json["identity"].get("email").is_none());
    assert!(
        json.get("role").is_none(),
        "membership role is not chrome data"
    );
    assert!(json.get("session").is_none());

    // Capabilities are server-resolved: a grant is visible on the next call.
    let registration = store::registration_for_email(&app.db, EMAIL)
        .await
        .expect("lookup")
        .expect("registered above");
    store::grant_capability(
        &app.db,
        registration.account_id,
        registration.identity_id,
        Capability::Manage,
    )
    .await
    .expect("grant manage");

    let after: Value = app.server.get("/api/whoami").await.json();
    assert_eq!(
        after["capabilities"],
        serde_json::json!(["test-auth", "manage"])
    );

    app.shutdown().await;
}
