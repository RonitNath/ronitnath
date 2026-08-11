//! The golden flow: interact → register → signin → interact → signout → interact.
//!
//! One session's whole life, asserted at every edge. The two `interact` legs
//! that bracket the signed-in one must fail, and the point of running the
//! *same* interaction three times is that the only thing changing between them
//! is the cookie — same router, same routes, same database.
//!
//! `/manage` rides along in every interaction and must fail all three times,
//! including the middle one where `/protected` succeeds. That is the assertion
//! that a session is not a blank cheque: `test-auth` is implied by every role and
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
//!
//! Capabilities have two sources, and each has a test that isolates it:
//! the role bundle (`role_change_takes_effect_on_the_next_request` — no rows,
//! effective immediately) and the exception row
//! (`manage_opens_once_the_capability_is_granted` — exactly one row, with a
//! `granted_at`). The golden flow itself asserts registration writes *no*
//! capability rows: the owner role is the grant.

mod common;

use axum::http::StatusCode;
use common::{TEST_PASSWORD, TestApp, timing};
use rn_site::auth::Capability;

/// How many explicit capability rows exist. Zero for every account whose
/// capabilities come purely from its role bundle.
async fn capability_rows(app: &TestApp) -> i64 {
    use hiqlite::params;

    #[derive(serde::Deserialize)]
    struct Count {
        n: i64,
    }

    app.db
        .query_as_one::<Count, _>(
            "SELECT COUNT(*) AS n FROM membership_capabilities",
            params!(),
        )
        .await
        .expect("count capability rows")
        .n
}

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

    // ── 2. register ── creates identity, primary account, and the owner
    // membership whose role implies `test-auth`. Explicitly does not sign
    // anyone in.
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

    // …and must not have written any capability rows either. `test-auth` in
    // leg 4 comes from the owner role's bundle; if a row appears here, the
    // registration path has regressed to direct grants.
    assert_eq!(
        capability_rows(&app).await,
        0,
        "registration must not write capability rows — the role is the grant"
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
        "`test-auth` is implied by every role, so /protected must open"
    );
    assert_eq!(
        during.manage,
        StatusCode::FORBIDDEN,
        "`manage` is implied by no role registration creates: /manage must be 403 (session read, \
         capability absent) and not 401 (no session)"
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
///
/// This is the *exception-row* path: the owner role's bundle does not carry
/// `manage`, so the grant must land as an auditable row.
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
        Capability::Manage,
    )
    .await
    .expect("grant manage");

    // Same cookie, same route: only the grant changed — and it is a row,
    // because it deviates from the role and must carry a `granted_at`.
    app.server.get("/manage").await.assert_status_ok();
    assert_eq!(
        capability_rows(&app).await,
        1,
        "an exception grant is exactly one auditable row"
    );

    app.shutdown().await;
}

/// Changing a membership's role re-derives its capabilities on the very next
/// request — no capability rows written, no session invalidated, no backfill.
///
/// This is the *bundle* path, and the reason the role is joined at resolve
/// time rather than copied into the session: promotion takes effect while the
/// session cookie stays exactly the same.
#[tokio::test]
async fn role_change_takes_effect_on_the_next_request() {
    use hiqlite::params;

    let app = TestApp::boot().await;
    let email = "promoted@example.test";

    app.server
        .post("/api/auth/register")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status(StatusCode::CREATED);
    app.server
        .post("/api/auth/signin")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await
        .assert_status_ok();

    // Owner: bundle carries `test-auth` only.
    app.server.get("/protected").await.assert_status_ok();
    app.server.get("/manage").await.assert_status_forbidden();

    // Promote the membership to admin, whose bundle carries `manage`. There is
    // no store function for this yet — an admin surface would own it — so the
    // test speaks SQL, exactly as that surface would.
    let updated = app
        .db
        .execute("UPDATE account_memberships SET role = 'admin'", params!())
        .await
        .expect("promote membership");
    assert_eq!(updated, 1, "exactly one membership to promote");

    // Same cookie: the bundle applies immediately, and no rows were involved.
    app.server.get("/manage").await.assert_status_ok();
    assert_eq!(
        capability_rows(&app).await,
        0,
        "a role bundle grants without writing capability rows"
    );

    // Demotion is the same lever in reverse — access ends with the role, which
    // is what makes the bundle a policy and not a one-time backfill.
    app.db
        .execute("UPDATE account_memberships SET role = 'member'", params!())
        .await
        .expect("demote membership");
    app.server.get("/manage").await.assert_status_forbidden();
    app.server.get("/protected").await.assert_status_ok();

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
