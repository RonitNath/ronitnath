//! The `/manage` data browser, end to end through the real router.
//!
//! Three properties are pinned here, in rising order of consequence:
//!
//! 1. **The guard matrix.** Anonymous → 401, signed-in without `manage` → 403,
//!    granted → 200 — for the index and for every model page, because each
//!    route is a separate registration and any one of them could be forgotten.
//! 2. **Redaction.** The pages render rows from tables that hold secrets; the
//!    full PHC string and the full session-token digest must not appear in any
//!    response body.
//! 3. **Escaping.** Emails and user agents are attacker-controlled and end up
//!    inside table cells; markup in them must come out inert.

mod common;

use axum::http::StatusCode;
use common::{TEST_PASSWORD, TestApp};
use hiqlite::params;
use rn_site::auth::{Capability, store};
use rn_site::manage::DataModel;

/// Register, sign in, and grant `manage` — the browser-ready operator session.
async fn signed_in_manager(app: &TestApp, email: &str) -> store::Registration {
    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);
    let registration = store::authenticate(&app.db, email, TEST_PASSWORD)
        .await
        .expect("authenticate query")
        .expect("credential is valid");
    store::grant_capability(
        &app.db,
        registration.account_id,
        registration.identity_id,
        Capability::Manage,
    )
    .await
    .expect("grant manage");
    app.server
        .post("/api/auth/signin")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status_ok();
    registration
}

#[tokio::test]
async fn every_manage_route_carries_the_guard() {
    let app = TestApp::boot().await;

    // Anonymous: 401 on the index and on every model page.
    app.server.get("/manage").await.assert_status_unauthorized();
    for model in DataModel::ALL {
        app.server
            .get(&format!("/manage/{model}"))
            .await
            .assert_status_unauthorized();
    }

    // Signed in without the capability: 403 everywhere. A fresh registration
    // holds `test-auth` only, so this is the default state of every new user.
    app.server
        .post("/api/auth/register")
        .form(&[
            ("email", "viewer@example.test"),
            ("password", TEST_PASSWORD),
        ])
        .await
        .assert_status(StatusCode::CREATED);
    app.server
        .post("/api/auth/signin")
        .form(&[
            ("email", "viewer@example.test"),
            ("password", TEST_PASSWORD),
        ])
        .await
        .assert_status_ok();
    app.server.get("/manage").await.assert_status_forbidden();
    for model in DataModel::ALL {
        app.server
            .get(&format!("/manage/{model}"))
            .await
            .assert_status_forbidden();
    }

    app.shutdown().await;
}

#[tokio::test]
async fn index_lists_every_model_and_pages_render_their_rows() {
    let app = TestApp::boot().await;
    let email = "operator@example.test";
    let registration = signed_in_manager(&app, email).await;

    let index = app.server.get("/manage").await;
    index.assert_status_ok();
    let body = index.text();
    for model in DataModel::ALL {
        assert!(
            body.contains(model.title()),
            "index is missing the {model} card"
        );
        assert!(
            body.contains(&format!("/manage/{model}")),
            "index is missing the link to {model}"
        );
    }

    // Every model page renders 200 with this account's data in place.
    for model in DataModel::ALL {
        app.server
            .get(&format!("/manage/{model}"))
            .await
            .assert_status_ok();
    }

    // The identity created above is visible by its public id — and only its
    // public id: the integer join keys must not be derivable from the page.
    let identities = app.server.get("/manage/identities").await.text();
    assert!(identities.contains(&registration.identity_public_id.to_string_lossy()));

    let emails = app.server.get("/manage/identity-emails").await.text();
    assert!(emails.contains(email));

    let memberships = app.server.get("/manage/account-memberships").await.text();
    assert!(memberships.contains("owner"));

    let capabilities = app
        .server
        .get("/manage/membership-capabilities")
        .await
        .text();
    assert!(capabilities.contains("manage"));

    app.shutdown().await;
}

#[tokio::test]
async fn unknown_model_is_a_styled_404() {
    let app = TestApp::boot().await;
    signed_in_manager(&app, "operator@example.test").await;

    let response = app.server.get("/manage/nope").await;
    response.assert_status(StatusCode::NOT_FOUND);
    assert!(response.text().contains("No such model"));

    app.shutdown().await;
}

/// The password page names the algorithm; the PHC string itself — salt, cost
/// parameters, output — must not be anywhere in the HTML.
#[tokio::test]
async fn password_hashes_never_reach_the_page() {
    let app = TestApp::boot().await;
    let registration = signed_in_manager(&app, "operator@example.test").await;

    #[derive(serde::Deserialize)]
    struct PhcRow {
        password_hash: String,
    }
    let phc: PhcRow = app
        .db
        .query_as_one(
            "SELECT password_hash FROM identity_passwords WHERE identity_id = $1",
            params!(registration.identity_id.get()),
        )
        .await
        .expect("password row exists");
    assert!(phc.password_hash.starts_with("$argon2id$"));

    let page = app.server.get("/manage/identity-passwords").await;
    page.assert_status_ok();
    let body = page.text();
    assert!(body.contains("argon2id"), "algorithm name is shown");
    assert!(
        !body.contains(&phc.password_hash),
        "full PHC string leaked into the page"
    );
    assert!(
        !body.contains("$argon2id$"),
        "PHC prefix indicates more than the algorithm name was rendered"
    );

    app.shutdown().await;
}

/// The sessions page shows an 8-character digest prefix — enough to correlate
/// with logs, not enough to look a row up in a leaked dump.
#[tokio::test]
async fn session_digests_are_truncated_on_the_page() {
    let app = TestApp::boot().await;
    signed_in_manager(&app, "operator@example.test").await;

    #[derive(serde::Deserialize)]
    struct HashRow {
        token_hash: String,
    }
    let hashes: Vec<HashRow> = app
        .db
        .query_as("SELECT token_hash FROM sessions", params!())
        .await
        .expect("session rows exist");
    assert!(!hashes.is_empty());

    let page = app.server.get("/manage/sessions").await;
    page.assert_status_ok();
    let body = page.text();
    for row in &hashes {
        assert!(
            body.contains(&row.token_hash[..8]),
            "digest prefix is shown"
        );
        assert!(
            !body.contains(&row.token_hash),
            "full token digest leaked into the page"
        );
    }

    app.shutdown().await;
}

/// Emails are attacker-controlled and rendered into table cells. The email
/// gate only requires an `@`, so markup fits through it — and must come out
/// escaped on every page that shows an address.
#[tokio::test]
async fn user_controlled_values_are_escaped() {
    let app = TestApp::boot().await;
    let hostile = "<img src=x onerror=alert(1)>@example.test";

    app.server
        .post("/api/auth/register")
        .form(&[("email", hostile), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);
    signed_in_manager(&app, "operator@example.test").await;

    for model in ["identity-emails", "accounts", "account-memberships"] {
        let body = app.server.get(&format!("/manage/{model}")).await.text();
        assert!(
            !body.contains("<img src=x"),
            "unescaped markup on /manage/{model}"
        );
    }
    let emails = app.server.get("/manage/identity-emails").await.text();
    assert!(emails.contains("&lt;img src=x onerror=alert(1)&gt;@example.test"));

    app.shutdown().await;
}

/// `membership_for_email` is what the admin CLI trusts to aim a grant.
#[tokio::test]
async fn membership_lookup_finds_the_primary_membership() {
    let app = TestApp::boot().await;
    let email = "target@example.test";

    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);
    let registration = store::authenticate(&app.db, email, TEST_PASSWORD)
        .await
        .expect("authenticate query")
        .expect("credential is valid");

    let membership = store::membership_for_email(&app.db, "TARGET@example.test")
        .await
        .expect("lookup query")
        .expect("membership exists; lookup normalizes case");
    assert_eq!(membership.0.get(), registration.account_id.get());
    assert_eq!(membership.1.get(), registration.identity_id.get());

    assert!(
        store::membership_for_email(&app.db, "absent@example.test")
            .await
            .expect("lookup query")
            .is_none()
    );

    app.shutdown().await;
}

#[tokio::test]
async fn references_render_as_labels_with_the_public_id_on_hover() {
    let app = TestApp::boot().await;
    let email = "operator@example.test";
    let registration = signed_in_manager(&app, email).await;
    let ipid = registration.identity_public_id.to_string_lossy();

    // Reference columns resolve server-side: the cell text is a human label
    // (here the primary email — no display name exists), and the full public
    // id rides in the title attribute for hover. No bare-uuid reference cells.
    for slug in ["identity-emails", "account-memberships", "sessions"] {
        let page = app.server.get(&format!("/manage/{slug}")).await.text();
        assert!(
            page.contains(&format!("title=\"{ipid}\"")),
            "{slug}: identity reference must carry its public id on hover"
        );
        assert!(
            page.contains(&format!("title=\"{ipid}\">{email}</span>")),
            "{slug}: identity reference must read as its label, not a uuid"
        );
    }

    // A resource grant's subject resolves the same way.
    rn_site::auth::grants::grant(
        &app.db,
        rn_site::auth::ResourceKind::Document,
        "11111111-2222-4333-8444-555555555555",
        rn_site::auth::GrantSubject::Identity(registration.identity_id),
        rn_site::auth::ShareRole::Viewer,
        registration.identity_id,
    )
    .await
    .expect("grant");
    let page = app.server.get("/manage/resource-grants").await.text();
    assert!(page.contains(&format!("title=\"{ipid}\">{email}</span>")));
    assert!(page.contains("viewer"));

    app.shutdown().await;
}
