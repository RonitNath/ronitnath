//! A product's routes appear and vanish while the process keeps running.
//!
//! This is the single-node half of B5's acceptance — the three-voter half is
//! `tools/cluster.sh` and a transcript, because "no node restarted" is a claim
//! about pids that a test harness inside one process cannot make.
//!
//! What it *can* make is the claim the transcript rests on: one `Router`, one
//! `TestServer`, built once at the top of each case and never rebuilt, and the
//! same URL answering `404`, then `200`, then `404` as the deployment's
//! decision moves. Everything goes through the real command over HTTP and the
//! real feed consumer — nothing here writes a `product` row or touches the
//! projection by hand, because a test that did would be proving a mechanism
//! the deployment does not have.

mod harness;

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum_test::TestServer;
use rn_kernel::product;
use serde_json::{Value, json};
use tokio::sync::OnceCell;

static NODE: OnceCell<rn_site::AppState> = OnceCell::const_new();

async fn state() -> rn_site::AppState {
    harness::node(&NODE, "127.0.0.1:8193", "127.0.0.1:8293").await
}

/// The route the catalogue's first product declares.
fn a_route() -> &'static str {
    product::CATALOGUE[0].mounts[0]
}

fn a_slug() -> &'static str {
    product::CATALOGUE[0].slug
}

/// Post a command as a signed-in caller.
async fn command(server: &TestServer, token: &str, name: &str, args: Value) -> axum_test::TestResponse {
    let mut body = args.as_object().cloned().unwrap_or_default();
    body.insert("key".to_owned(), json!(uuid::Uuid::new_v4().to_string()));
    server
        .post(&format!("/api/cmd/{name}"))
        .add_header("cookie", format!("rn_session={token}"))
        .add_header("sec-fetch-site", "same-origin")
        .text(Value::Object(body).to_string())
        .content_type("application/json")
        .await
}

/// Wait for this node's projection to catch up with a decision it has already
/// committed — the feed consumer is a task, so "the command returned" and
/// "this node has re-read the table" are two moments.
async fn until(server: &TestServer, path: &str, want: StatusCode) -> StatusCode {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let seen = server.get(path).await.status_code();
        if seen == want || Instant::now() >= deadline {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[test]
fn a_product_route_answers_only_while_the_product_is_on() {
    harness::run(async {
        let state = state().await;
        // One router, built once, and never rebuilt for the rest of this case.
        let server = harness::server(&state);
        let path = a_route();

        // Default-deny: no row, so the route is a 404 — the same answer a path
        // this deployment never had gives.
        assert_eq!(server.get(path).await.status_code(), StatusCode::NOT_FOUND);

        let operator = harness::register(&state, "Operator", "products-op@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;

        let enabled = command(
            &server,
            &operator.token,
            "enable-product",
            json!({ "slug": a_slug() }),
        )
        .await;
        assert_eq!(
            enabled.status_code(),
            StatusCode::OK,
            "enable-product answered {}: {}",
            enabled.status_code(),
            enabled.text()
        );

        assert_eq!(
            until(&server, path, StatusCode::OK).await,
            StatusCode::OK,
            "the product's route did not start answering"
        );

        let disabled = command(
            &server,
            &operator.token,
            "disable-product",
            json!({ "slug": a_slug() }),
        )
        .await;
        assert_eq!(disabled.status_code(), StatusCode::OK);

        assert_eq!(
            until(&server, path, StatusCode::NOT_FOUND).await,
            StatusCode::NOT_FOUND,
            "the product's route kept answering after it was turned off"
        );
    });
}

#[test]
fn a_member_who_is_not_an_operator_cannot_turn_a_product_on() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let member = harness::register(&state, "Member", "products-member@example.invalid").await;

        let refused = command(
            &server,
            &member.token,
            "enable-product",
            json!({ "slug": a_slug() }),
        )
        .await;
        // The uniform decline a command route gives, which is the same one an
        // operator's own commands give a stranger. What the route answers is
        // not asserted here: this binary's cases share one node, and whether a
        // product is on at this instant is the other case's business.
        assert_eq!(refused.status_code(), StatusCode::FORBIDDEN);
    });
}

#[test]
fn a_slug_this_build_does_not_carry_is_declined() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let operator =
            harness::register(&state, "Operator", "products-typo@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;

        let refused = command(
            &server,
            &operator.token,
            "enable-product",
            json!({ "slug": "events" }),
        )
        .await;
        assert_eq!(
            refused.status_code(),
            StatusCode::FORBIDDEN,
            "a typo in a slug is a refusal, not a row nothing reads"
        );
    });
}
