//! The dev bypass: a credential-free `manage` session in debug builds only.
//!
//! This whole test file exercises code that exists only under
//! `#[cfg(debug_assertions)]` — which is exactly what `cargo test` builds. The
//! release-build property (the route does not exist at all) is a compile-time
//! `cfg` on the route registration and the handler; there is no flag to test
//! because there is nothing to disable.
//!
//! What *is* testable in a debug build is the runtime gate: a prod-mode
//! process answers 404, indistinguishable from the route being absent.

#![cfg(debug_assertions)]

mod common;

use axum::http::{StatusCode, header};
use common::TestApp;
use rn_site::operations::config::Mode;

/// One click: session cookie, redirect to `/manage`, and `/manage` opens.
#[tokio::test]
async fn bypass_mints_a_manage_capable_session_in_dev_mode() {
    let app = TestApp::boot().await;

    // Anonymous, no models visible.
    app.server.get("/manage").await.assert_status_unauthorized();

    let response = app.server.post("/api/auth/dev-bypass").await;
    response.assert_status(StatusCode::SEE_OTHER);
    assert_eq!(response.header(header::LOCATION), "/manage");
    assert!(
        response
            .header(header::SET_COOKIE)
            .to_str()
            .expect("ascii cookie")
            .starts_with("rn_session="),
        "bypass must install a session cookie"
    );

    // The redirect target opens with the minted session (cookie jar carried it).
    app.server.get("/manage").await.assert_status_ok();
    app.server.get("/protected").await.assert_status_ok();

    // A second click mints a second session for the same identity rather than
    // failing on "already registered".
    app.server
        .post("/api/auth/dev-bypass")
        .await
        .assert_status(StatusCode::SEE_OTHER);

    app.shutdown().await;
}

/// Prod configuration hides the route even though this is a debug binary:
/// 404, exactly what a release build — where the route is not compiled — says.
#[tokio::test]
async fn bypass_answers_not_found_in_prod_mode() {
    let app = TestApp::boot_in_mode(Mode::Prod).await;

    app.server
        .post("/api/auth/dev-bypass")
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // And nothing was minted along the way.
    app.server.get("/manage").await.assert_status_unauthorized();

    app.shutdown().await;
}
