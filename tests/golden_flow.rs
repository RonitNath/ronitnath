//! The golden flow: interact → register → signin → interact → signout → interact.
//!
//! One session's whole life, asserted at every edge. The two `interact` legs
//! that bracket the signed-in one must fail, and the point of running the
//! *same* interaction three times is that the only thing changing between them
//! is the cookie — same router, same routes, same database.
//!
//! `/manage` rides along in every interaction and must fail all three times,
//! including the middle one where `/protected` succeeds. That is the assertion
//! that a session is not a blank cheque: `test-auth` is a default grant and
//! `manage` is not, so a signed-in account clears one guard and not the other.
//!
//! The failure *codes* carry the distinction that matters:
//!
//! | leg           | `/protected` | `/manage` |
//! |---------------|--------------|-----------|
//! | before signin | 401          | 401       |
//! | signed in     | 200          | **403**   |
//! | after signout | 401          | 401       |
//!
//! 403 in the middle row rather than 401 is the whole result: the session was
//! real and was read, and the capability was still missing.

mod common;

use axum::http::StatusCode;
use common::{TEST_PASSWORD, TestApp, timing};
use rn_site::auth::{MANAGE, TEST_AUTH};

/// What one "interact" leg observed.
#[derive(Debug, PartialEq, Eq)]
struct Interaction {
    protected: StatusCode,
    manage: StatusCode,
}

/// Hit both guarded pages once. Nothing here knows whether a session exists —
/// that is what makes the three call sites comparable.
async fn interact(app: &TestApp, leg: &str) -> Interaction {
    let protected = timing::step(&format!("{leg}.protected"), async {
        app.server.get("/protected").await
    })
    .await;

    let manage = timing::step(&format!("{leg}.manage"), async {
        app.server.get("/manage").await
    })
    .await;

    let observed = Interaction {
        protected: protected.status_code(),
        manage: manage.status_code(),
    };
    tracing::info!(leg, ?observed, "interaction observed");
    observed
}

#[tokio::test]
async fn golden_flow_register_signin_signout() {
    let app = TestApp::boot().await;
    let email = "golden@example.test";

    // ── 1. interact, anonymous ── both guards reject; no session exists yet.
    let before = interact(&app, "1.anon").await;
    assert_eq!(
        before,
        Interaction {
            protected: StatusCode::UNAUTHORIZED,
            manage: StatusCode::UNAUTHORIZED,
        },
        "an anonymous caller must be turned away from both guarded pages"
    );

    // ── 2. register ── creates identity, primary account, membership, and the
    // default grants. Explicitly does not sign anyone in.
    let registered = timing::step("2.register", async {
        app.server
            .post("/api/auth/register")
            .form(&[("email", email), ("password", TEST_PASSWORD)])
            .await
    })
    .await;
    registered.assert_status(StatusCode::CREATED);

    // Registering must not have handed out a session as a side effect.
    assert!(
        registered
            .maybe_cookie(rn_site::auth::routes::COOKIE_NAME)
            .is_none(),
        "register must not establish a session"
    );
    let still_out = interact(&app, "2.after-register").await;
    assert_eq!(
        still_out.protected,
        StatusCode::UNAUTHORIZED,
        "registering is not signing in"
    );

    // ── 3. signin ── the response's Set-Cookie is what every later request
    // rides on, via the test server's cookie jar.
    let signed_in = timing::step("3.signin", async {
        app.server
            .post("/api/auth/signin")
            .form(&[("email", email), ("password", TEST_PASSWORD)])
            .await
    })
    .await;
    signed_in.assert_status_ok();
    signed_in.assert_contains_cookie(rn_site::auth::routes::COOKIE_NAME);

    // ── 4. interact, signed in ── the leg that separates the two capabilities.
    let during = interact(&app, "4.signed-in").await;
    assert_eq!(
        during.protected,
        StatusCode::OK,
        "`{TEST_AUTH}` is a default grant, so /protected must open"
    );
    assert_eq!(
        during.manage,
        StatusCode::FORBIDDEN,
        "`{MANAGE}` is not a default grant: /manage must be 403 (session read, capability absent) \
         and not 401 (no session)"
    );

    // ── 5. signout ── revokes the row and clears the cookie.
    let signed_out = timing::step("5.signout", async {
        app.server.post("/api/auth/signout").await
    })
    .await;
    signed_out.assert_status_ok();
    let cleared = signed_out.cookie(rn_site::auth::routes::COOKIE_NAME);
    assert_eq!(
        cleared.value(),
        "",
        "signout must send an empty session cookie"
    );

    // ── 6. interact, anonymous again ── identical to leg 1. If this passed
    // only because the client dropped the cookie, `revoke_session_survives_a
    // _replayed_cookie` below is the test that would still catch it.
    let after = interact(&app, "6.anon").await;
    assert_eq!(
        after, before,
        "after signout the caller must be exactly as unauthorized as before signin"
    );

    app.shutdown().await;
}

/// A revoked token must fail even when it is replayed by hand.
///
/// The golden flow's last leg only proves the *client* forgot the cookie. This
/// proves the *server* forgot the session, which is the property sign-out is
/// actually supposed to have.
#[tokio::test]
async fn revoked_session_is_rejected_when_the_cookie_is_replayed() {
    let app = TestApp::boot().await;
    let email = "replay@example.test";

    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);

    let signed_in = app
        .server
        .post("/api/auth/signin")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await;
    signed_in.assert_status_ok();
    let stolen = signed_in
        .cookie(rn_site::auth::routes::COOKIE_NAME)
        .value()
        .to_string();

    app.server.get("/protected").await.assert_status_ok();
    app.server
        .post("/api/auth/signout")
        .await
        .assert_status_ok();

    // Replay the exact cookie the browser was given before signing out.
    app.server
        .get("/protected")
        .add_header(
            axum::http::header::COOKIE,
            format!("{}={stolen}", rn_site::auth::routes::COOKIE_NAME),
        )
        .await
        .assert_status_unauthorized();

    app.shutdown().await;
}

/// Sign-out deletes the row rather than flagging it.
///
/// The replay test above proves the session stops working; this proves *how*.
/// Without it, reintroducing a tombstone column would go unnoticed — the
/// observable behaviour is identical right up until someone dumps the table.
#[tokio::test]
async fn signout_deletes_the_session_row() {
    use hiqlite::params;

    #[derive(serde::Deserialize)]
    struct Count {
        n: i64,
    }

    let app = TestApp::boot().await;
    let email = "deleted@example.test";

    async fn sessions(app: &TestApp) -> i64 {
        app.db
            .query_as_one::<Count, _>("SELECT COUNT(*) AS n FROM sessions", params!())
            .await
            .expect("count sessions")
            .n
    }

    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);
    assert_eq!(
        sessions(&app).await,
        0,
        "register must not create a session"
    );

    app.server
        .post("/api/auth/signin")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status_ok();
    assert_eq!(sessions(&app).await, 1);

    app.server
        .post("/api/auth/signout")
        .await
        .assert_status_ok();
    assert_eq!(
        sessions(&app).await,
        0,
        "revocation must remove the row, not tombstone it"
    );

    // And doing it again is still a clean 200 with nothing left to delete.
    app.server
        .post("/api/auth/signout")
        .await
        .assert_status_ok();
    assert_eq!(sessions(&app).await, 0);

    app.shutdown().await;
}

/// Only a one-way digest belongs in persistent state; the browser's bearer
/// token must never be recoverable from a database or backup.
#[tokio::test]
async fn database_stores_only_the_session_token_digest() {
    use hiqlite::params;

    #[derive(serde::Deserialize)]
    struct StoredSession {
        token_hash: String,
    }

    let app = TestApp::boot().await;
    let email = "digest@example.test";
    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);

    let signed_in = app
        .server
        .post("/api/auth/signin")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await;
    let token = signed_in
        .cookie(rn_site::auth::routes::COOKIE_NAME)
        .value()
        .to_string();
    let stored: StoredSession = app
        .db
        .query_as_one("SELECT token_hash FROM sessions", params!())
        .await
        .expect("read session digest");

    assert_ne!(stored.token_hash, token);
    assert_eq!(
        stored.token_hash,
        rn_site::auth::session::token_hash(&token)
    );
    assert_eq!(stored.token_hash.len(), 64);

    app.shutdown().await;
}

/// A session is replicated database state, not process memory or a key loaded
/// at boot. Reopening the entire application must accept the browser's same
/// bearer token until its normal expiry or revocation.
#[tokio::test]
async fn session_survives_a_complete_application_restart() {
    let app = TestApp::boot().await;
    let email = "restart@example.test";
    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);
    let signed_in = app
        .server
        .post("/api/auth/signin")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await;
    let token = signed_in
        .cookie(rn_site::auth::routes::COOKIE_NAME)
        .value()
        .to_string();

    let app = app.restart().await;
    app.server
        .get("/protected")
        .add_header(
            axum::http::header::COOKIE,
            format!("{}={token}", rn_site::auth::routes::COOKIE_NAME),
        )
        .await
        .assert_status_ok();
    app.shutdown().await;
}

/// Auth and authorization responses are user-specific and may carry a bearer
/// cookie; neither browsers nor intermediaries should retain them.
#[tokio::test]
async fn auth_responses_are_not_cacheable() {
    use axum::http::header;

    let app = TestApp::boot().await;
    let response = app.server.get("/protected").await;
    response.assert_status_unauthorized();
    assert_eq!(response.header(header::CACHE_CONTROL), "no-store");
    assert_eq!(response.header(header::PRAGMA), "no-cache");
    app.shutdown().await;
}

/// `/manage` opens once `manage` is granted — proving the 403 in the golden
/// flow is the capability check and not a permanently broken route.
#[tokio::test]
async fn manage_opens_once_the_capability_is_granted() {
    let app = TestApp::boot().await;
    let email = "manager@example.test";

    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);

    let registration = rn_site::auth::store::authenticate(&app.db, email, TEST_PASSWORD)
        .await
        .expect("authenticate query")
        .expect("credential is valid");

    app.server
        .post("/api/auth/signin")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status_ok();

    app.server.get("/manage").await.assert_status_forbidden();

    rn_site::auth::store::grant_capability(
        &app.db,
        registration.account_id,
        registration.identity_id,
        MANAGE,
    )
    .await
    .expect("grant manage");

    // Same cookie, same route: only the grant changed.
    app.server.get("/manage").await.assert_status_ok();

    app.shutdown().await;
}

/// Wrong password and unknown address must be indistinguishable to the caller.
#[tokio::test]
async fn declines_are_uniform() {
    let app = TestApp::boot().await;
    let email = "uniform@example.test";

    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);

    let wrong_password = app
        .server
        .post("/api/auth/signin")
        .form(&[("email", email), ("password", "not the password")])
        .await;
    let unknown_address = app
        .server
        .post("/api/auth/signin")
        .form(&[
            ("email", "nobody@example.test"),
            ("password", TEST_PASSWORD),
        ])
        .await;

    wrong_password.assert_status_unauthorized();
    unknown_address.assert_status_unauthorized();
    assert_eq!(
        wrong_password.text(),
        unknown_address.text(),
        "a wrong password and an unknown address must produce byte-identical bodies"
    );
    assert!(
        wrong_password
            .maybe_cookie(rn_site::auth::routes::COOKIE_NAME)
            .is_none(),
        "a declined sign-in must not set a session cookie"
    );

    // Registering the same address twice is a conflict, not a second identity.
    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CONFLICT);

    app.shutdown().await;
}
