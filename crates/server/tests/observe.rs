//! The observation lane, end to end: a timer that runs, a queue that empties,
//! and a signal that reaches the match queue without a request paying for it.
//!
//! This suite boots its own node on a *fixed* clock, because the whole point
//! of the lane is that it is late on purpose: `last_seen_at` is written at
//! most once per `THROTTLE` and the throttle is enforced in the statement, so
//! proving a second write happens means being five minutes later than the
//! first one. Waiting five minutes is not a test.

mod harness;

use std::time::Duration;

use rn_kernel::ids::{Id, Identity};
use rn_kernel::observe::THROTTLE;
use rn_kernel::store::{Clock, Count, Reads};
use rn_kernel::{Principal, bind};
use rn_site::state::AppState;
use serde_json::json;
use tokio::sync::OnceCell;

static NODE: OnceCell<AppState> = OnceCell::const_new();

/// These cases share one node *and one clock*, and each of them moves the
/// world: a drain another case started would write the row this one is
/// watching. So they run one at a time, which is what "the lane is a
/// singleton" means when you are testing it rather than running it.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn alone() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The second this suite's world starts at. Any plausible unix time will do;
/// what matters is that the tests move it and the system clock cannot.
const EPOCH: i64 = 1_800_000_000;

async fn state() -> AppState {
    harness::node_on(
        &NODE,
        "127.0.0.1:8194",
        "127.0.0.1:8294",
        Clock::fixed(EPOCH),
    )
    .await
}

async fn last_seen(state: &AppState, identity: Id<Identity>) -> i64 {
    state
        .store
        .reads()
        .query::<Count>(
            "SELECT last_seen_at AS n FROM session WHERE identity_id = $1",
            bind![identity],
        )
        .await
        .expect("the session's last sighting")[0]
        .0
}

async fn candidates(state: &AppState) -> i64 {
    state
        .store
        .reads()
        .query::<Count>(
            "SELECT count(*) AS n FROM match_candidate WHERE status = 'proposed'",
            bind![],
        )
        .await
        .expect("the match queue")[0]
        .0
}

#[test]
fn a_sighting_the_request_path_refused_to_write_is_written_by_the_drain() {
    let _alone = alone();
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let mine = harness::register(&state, "Seen", "observe-seen@example.invalid").await;
        let cookie = format!("rn_session={}", mine.token);
        let created = last_seen(&state, mine.identity).await;

        // A read enqueues a sighting and writes nothing. That is the rule the
        // lane exists to keep, so it is worth asserting rather than assuming.
        server
            .get("/api/whoami")
            .add_header("cookie", cookie.clone())
            .await
            .assert_status_ok();
        assert!(
            !state.observations.is_empty(),
            "an authenticated read left nothing on the lane"
        );
        assert_eq!(
            last_seen(&state, mine.identity).await,
            created,
            "a GET wrote to the session row"
        );

        // Inside the throttle window the drain declines to write, and the
        // queue empties either way — which is what makes the console's
        // \"Observations pending\" fall back to zero rather than only climb.
        assert_eq!(rn_site::observe::tick(&state).await.seen, 0);
        assert_eq!(state.observations.len(), 0);
        assert_eq!(last_seen(&state, mine.identity).await, created);

        // Five minutes later, the same sighting is a write.
        state.store.clock().advance(THROTTLE + 1);
        server
            .get("/api/whoami")
            .add_header("cookie", cookie)
            .await
            .assert_status_ok();
        assert_eq!(rn_site::observe::tick(&state).await.seen, 1);
        assert!(
            last_seen(&state, mine.identity).await >= created + THROTTLE,
            "the sighting did not advance the session's last_seen_at"
        );
        assert_eq!(state.observations.len(), 0);
    });
}

#[test]
fn two_identities_that_proved_the_same_oidc_subject_are_proposed_as_one_person() {
    let _alone = alone();
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        // The lane on a short interval: the same task `main` starts, asked to
        // tick often enough for a test to watch it happen.
        let lane = rn_site::observe::spawn_every(state.clone(), Duration::from_millis(20));

        let one = harness::register(&state, "Doubled", "observe-oidc-a@example.invalid").await;
        let two = harness::register(&state, "Doubled", "observe-oidc-b@example.invalid").await;
        let before = candidates(&state).await;

        // `oidc` is a schema kind with no command behind it (`AddFactor`
        // refuses one), so the two registrations are given the shared subject
        // as rows — exactly as an OIDC sign-in would write them.
        for identity in [one.identity, two.identity] {
            state
                .store
                .execute(
                    "INSERT INTO factor (identity_id, kind, value, created_at, verified_at) \
                     VALUES ($1, 'oidc', 'https://issuer.example/sub/shared', $2, $2)",
                    bind![identity, EPOCH],
                )
                .await
                .expect("an oidc factor");
        }

        // A real command that changes what one of them has proven. The lane
        // reads it off the change feed; nothing on the request path scans.
        let response = server
            .post("/api/cmd/add-factor")
            .add_header("cookie", format!("rn_session={}", one.token))
            .add_header("sec-fetch-site", "same-origin")
            .json(&json!({
                "key": uuid::Uuid::new_v4().to_string(),
                "kind": "email",
                "value": "observe-oidc-a-second@example.invalid",
            }))
            .await;
        response.assert_status_ok();

        let mut proposed = candidates(&state).await;
        for _ in 0..200 {
            if proposed > before {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
            proposed = candidates(&state).await;
        }
        assert!(
            proposed > before,
            "the lane never proposed the pair it was told about"
        );
        lane.stop().await;
    });
}

#[test]
fn the_lane_stops_when_it_is_told_to() {
    let _alone = alone();
    harness::run(async {
        let state = state().await;
        let lane = rn_site::observe::spawn_every(state.clone(), Duration::from_millis(5));
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(lane.is_running(), "the lane never started");
        // What `main` does on the way out, and the half that matters: the task
        // is gone before the database is handed back.
        lane.stop().await;
    });
}

/// The principal a bare registration resolves to, asserted once so the two
/// suites above can trust `harness::register`.
#[test]
fn a_registration_on_this_node_is_a_member() {
    let _alone = alone();
    harness::run(async {
        let state = state().await;
        let who = harness::register(&state, "Member", "observe-member@example.invalid").await;
        let token = rn_kernel::domain::Token::from_wire(&who.token);
        let resolved = rn_kernel::principal::resolve(&state.store.reads(), &token)
            .await
            .expect("the session resolves");
        assert!(matches!(resolved.principal, Principal::Member { .. }));
    });
}
