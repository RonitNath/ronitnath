//! `POST /auth/dev` — the developer sign-in, and the two gates on it.
//!
//! The compile gate is not tested here, because it cannot be: this binary is a
//! debug build by construction, so the only thing a test can say about the
//! release profile is what `crates/server/tests/surface.rs` says — the route
//! answers `404`, and does so in both profiles for two different reasons.
//! What is proven here is everything else: that the session it opens is a real
//! operator session with a real audit row, that pressing the button twice does
//! not accumulate operators, and that the runtime gate closes on either half.

mod harness;

use rn_kernel::bind;
use rn_kernel::store::{Count, Reads};
use rn_site::config::{AppConfig, Mode};
use rn_site::state::AppState;
use tokio::sync::OnceCell;

/// The address the bypass invents when a deployment has no operator.
const DEV_EMAIL: &str = "dev-operator@example.invalid";

static ON: OnceCell<AppState> = OnceCell::const_new();
static OFF: OnceCell<AppState> = OnceCell::const_new();

/// `RN_SITE__DEV=1`, in dev mode: the only configuration that serves it.
async fn on() -> AppState {
    harness::node_dev(&ON, "127.0.0.1:8204", "127.0.0.1:8304", Mode::Dev, true).await
}

/// A dev process that was not told. The route is compiled in and answers as
/// though it is not.
async fn off() -> AppState {
    harness::node_dev(&OFF, "127.0.0.1:8205", "127.0.0.1:8305", Mode::Dev, false).await
}

/// The flag set on a production process — the mistake the second half of the
/// gate exists for.
///
/// The mode is moved on a state that is already serving rather than on a node
/// booted in it: `mode=prod` demands the cluster topology `RN_SITE_HQL_*`
/// carries, so a production process is not a thing a single-node test binary
/// can boot. What the gate reads is the configuration, and this is that
/// configuration on the same database, so nothing about the check is faked.
async fn prod() -> AppState {
    let serving = off().await;
    let mut state = serving.clone();
    state.config = std::sync::Arc::new(AppConfig {
        mode: Mode::Prod,
        dev: true,
        ..(*serving.config).clone()
    });
    state
}

/// How many people operate this deployment, and how many are registered at the
/// invented address. Both are counted because pressing twice must move
/// neither.
async fn counts(state: &AppState) -> (i64, i64) {
    let reads = state.store.reads();
    let operators = reads
        .query::<Count>(
            "SELECT count(*) AS n FROM relation \
             WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator'",
            bind![],
        )
        .await
        .expect("the operator relation")[0]
        .0;
    let people = reads
        .query::<Count>(
            "SELECT count(*) AS n FROM factor WHERE kind = 'email' AND value = $1",
            bind![DEV_EMAIL],
        )
        .await
        .expect("the dev registration")[0]
        .0;
    (operators, people)
}

/// The `rn_session` value out of a response's `Set-Cookie`.
fn session_cookie(response: &axum_test::TestResponse) -> String {
    let raw = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .expect("the reply sets a cookie");
    assert!(raw.contains("HttpOnly"), "{raw}");
    assert!(raw.contains("SameSite=Lax"), "{raw}");
    raw.strip_prefix("rn_session=")
        .and_then(|rest| rest.split(';').next())
        .expect("an rn_session cookie")
        .to_owned()
}

#[test]
fn the_button_opens_a_real_operator_session_and_the_second_press_reuses_it() {
    harness::run(async {
        let state = on().await;
        let server = harness::server(&state);

        // The page offers it, and the button posts to the route.
        let page = server.get("/auth").await;
        page.assert_status_ok();
        let html = page.text();
        assert!(html.contains("Sign in as operator"), "{html}");
        assert!(html.contains(r#"action="/auth/dev""#), "{html}");

        assert_eq!(counts(&state).await, (0, 0), "nothing exists yet");

        let pressed = server
            .post("/auth/dev")
            .add_header("sec-fetch-site", "same-origin")
            .await;
        assert_eq!(pressed.status_code(), 303);
        assert_eq!(
            pressed
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok()),
            Some("/platform"),
            "the bypass lands on the tier it granted"
        );
        let cookie = session_cookie(&pressed);

        // The session is a session: it resolves, it names somebody, and the
        // tier it carries is the operator relation rather than a flag.
        let whoami = server
            .get("/api/whoami")
            .add_header("cookie", format!("rn_session={cookie}"))
            .await;
        whoami.assert_status_ok();
        let body: serde_json::Value = whoami.json();
        let tiers = body["tiers"].as_array().expect("the tiers").clone();
        assert!(
            tiers.iter().any(|tier| tier == "platform"),
            "the developer session does not operate the platform: {body}"
        );

        // And `/platform` opens for it, which is what the relation *is*.
        let shell = server
            .get("/platform")
            .add_header("cookie", format!("rn_session={cookie}"))
            .await;
        assert_eq!(shell.status_code(), 200);

        // The audit tail says how this session was opened. `sign-in` would
        // have been a lie about a password nobody typed.
        let audit = server
            .get("/api/q/audit")
            .add_header("cookie", format!("rn_session={cookie}"))
            .await;
        audit.assert_status_ok();
        let text = audit.text();
        assert!(text.contains("dev-sign-in"), "{text}");

        assert_eq!(
            counts(&state).await,
            (1, 1),
            "one operator, one registration"
        );

        // Pressing it again signs in as the same operator. The kernel refuses
        // a second `platform:* #operator`, so a bypass that registered a
        // second person on every press would lock itself out of the tier.
        let again = server
            .post("/auth/dev")
            .add_header("sec-fetch-site", "same-origin")
            .await;
        assert_eq!(again.status_code(), 303);
        let second = session_cookie(&again);
        assert_ne!(second, cookie, "each press mints its own session");
        assert_eq!(
            counts(&state).await,
            (1, 1),
            "no second person and no second operator relation"
        );

        let whoami = server
            .get("/api/whoami")
            .add_header("cookie", format!("rn_session={second}"))
            .await;
        whoami.assert_status_ok();
        let second_body: serde_json::Value = whoami.json();
        assert_eq!(
            second_body["person"], body["person"],
            "the second press is the same operator"
        );

        // And it grants nobody it did not have to. Somebody else registering
        // afterwards is not an operator, and pressing again does not make
        // them one — the deployment's operator is settled.
        let bystander = harness::register(&state, "Bystander", "dev-bystander@example.invalid")
            .await
            .person;
        let third = server
            .post("/auth/dev")
            .add_header("sec-fetch-site", "same-origin")
            .await;
        assert_eq!(third.status_code(), 303);
        let held = state
            .store
            .reads()
            .query::<Count>(
                "SELECT count(*) AS n FROM relation \
                 WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator' \
                   AND subject_kind = 'person' AND subject_id = $1",
                bind![bystander],
            )
            .await
            .expect("the bystander's relations")[0]
            .0;
        assert_eq!(held, 0);
        assert_eq!(counts(&state).await, (1, 1), "still exactly one operator");
    });
}

#[test]
fn a_process_that_was_not_told_dev_has_no_such_route_and_no_such_button() {
    harness::run(async {
        let state = off().await;
        let server = harness::server(&state);

        let page = server.get("/auth").await;
        page.assert_status_ok();
        let html = page.text();
        assert!(!html.contains("Sign in as operator"), "{html}");
        assert!(!html.contains("/auth/dev"), "{html}");

        // Every way of asking, including the two that would otherwise be
        // refused by the same-origin guard with a `403` — which would say the
        // route is there.
        for site in ["same-origin", "cross-site", "same-site"] {
            let response = server
                .post("/auth/dev")
                .add_header("sec-fetch-site", site)
                .await;
            assert_eq!(response.status_code(), 404, "as {site}");
        }
        let silent = server.post("/auth/dev").await;
        assert_eq!(silent.status_code(), 404);

        assert_eq!(counts(&state).await, (0, 0), "and it wrote nothing");
    });
}

#[test]
fn the_flag_alone_does_not_open_it_on_a_production_process() {
    harness::run(async {
        let state = prod().await;
        assert!(state.config.dev, "the flag is set and it is not enough");
        let server = harness::server(&state);

        let response = server
            .post("/auth/dev")
            .add_header("sec-fetch-site", "same-origin")
            .await;
        assert_eq!(response.status_code(), 404);

        let page = server.get("/auth").await;
        page.assert_status_ok();
        assert!(!page.text().contains("Sign in as operator"));

        assert_eq!(counts(&state).await, (0, 0));
    });
}
