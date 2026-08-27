//! The `/platform` queries: who they answer, and what their statements cost.
//!
//! Two claims, and they are the two ways this surface could be wrong.
//!
//! **Who.** Every platform query is the whole deployment, so the operator
//! relation is re-read on every request. A member and an organization admin
//! are refused by all of them, with the same bytes an unknown query name gets
//! — telling those apart is how a caller learns what exists.
//!
//! **What it costs.** These lists are unbounded by the caller's own behaviour:
//! their size is the deployment's. So every one of their statements goes under
//! `EXPLAIN QUERY PLAN` here, exactly as the kernel's hot queries do, and a
//! plan that scans a table it should have sought is a failure rather than
//! something that shows up on a cluster with a million rows in it.

mod harness;

use axum::http::StatusCode;
use rn_kernel::bind;
use rn_kernel::testing::Local;
use rn_site::api::query::{ALL, Named};
use tokio::sync::OnceCell;

static NODE: OnceCell<rn_site::AppState> = OnceCell::const_new();

async fn state() -> rn_site::AppState {
    harness::node(&NODE, "127.0.0.1:8195", "127.0.0.1:8295").await
}

/// Every platform query name, as a URL.
fn platform_names() -> Vec<&'static str> {
    ALL.iter()
        .filter_map(|query| match query {
            Named::Platform(_) => Some(query.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn every_platform_query_is_reachable_and_named_once() {
    let names = platform_names();
    assert_eq!(names.len(), 10, "the platform surface is ten queries");
    for name in &names {
        assert!(name.starts_with("platform-"), "{name}");
        assert_eq!(Named::parse(name).map(|q| q.as_str()), Some(*name));
    }
}

#[test]
fn a_member_and_an_org_admin_decline_every_platform_query() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);

        let member = harness::register(&state, "Member", "plat-member@example.invalid").await;
        let org = harness::register(&state, "OrgAdmin", "plat-org@example.invalid").await;
        harness::operate_organization(&state, org.person, "Platform Matrix Organization").await;
        let operator = harness::register(&state, "Operator", "plat-operator@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;

        // A subject the drill-ins can be asked about, so a decline is a
        // decline about authorisation rather than about a missing parameter.
        let subject = {
            let response = server
                .get("/api/whoami")
                .add_header("cookie", format!("rn_session={}", operator.token))
                .await;
            response.assert_status_ok();
            let body: serde_json::Value = response.json();
            body["person"]["public_id"]
                .as_str()
                .expect("a person id")
                .to_owned()
        };

        for name in platform_names() {
            let url = format!("/api/q/{name}?subject={subject}");
            for (who, token) in [
                ("anonymous", None),
                ("a member", Some(&member.token)),
                ("an organization admin", Some(&org.token)),
            ] {
                let mut request = server.get(&url);
                if let Some(token) = token {
                    request = request.add_header("cookie", format!("rn_session={token}"));
                }
                let response = request.await;
                assert_eq!(
                    response.status_code(),
                    StatusCode::FORBIDDEN,
                    "{name} answered {who} with {}",
                    response.status_code()
                );
            }

            let response = server
                .get(&url)
                .add_header("cookie", format!("rn_session={}", operator.token))
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::OK,
                "{name} refused the operator — body {}",
                response.text()
            );
            assert!(
                response.json::<serde_json::Value>().is_array(),
                "{name} did not answer with a result set"
            );
        }

        // And the node's own account of itself is guarded the same way.
        let cluster = server
            .get("/api/q/cluster")
            .add_header("cookie", format!("rn_session={}", member.token))
            .await;
        assert_eq!(cluster.status_code(), StatusCode::FORBIDDEN);
        let cluster = server
            .get("/api/q/cluster")
            .add_header("cookie", format!("rn_session={}", operator.token))
            .await;
        cluster.assert_status_ok();
        let view: rn_api::ClusterView = cluster.json();
        assert!(view.raft.sqlite.healthy, "the test node's db raft is up");
        assert!(view.raft.cache.healthy, "the test node's cache raft is up");
        assert!(view.feed_head > 0, "registrations landed on the feed");
    });
}

#[test]
fn an_operator_who_loses_the_relation_loses_the_surface_on_the_next_request() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let operator = harness::register(&state, "Fleeting", "plat-fleeting@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;
        let cookie = format!("rn_session={}", operator.token);

        server
            .get("/api/q/platform-parties")
            .add_header("cookie", cookie.clone())
            .await
            .assert_status_ok();

        // The relation is the authorisation, so taking the row away takes the
        // surface with it — without a sign-out, and without waiting for a
        // cached tier to expire.
        state
            .store
            .execute(
                "DELETE FROM relation WHERE object_kind = 'platform' AND subject_id = $1",
                bind![operator.person],
            )
            .await
            .expect("the row goes");

        let response = server
            .get("/api/q/platform-parties")
            .add_header("cookie", cookie)
            .await;
        assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    });
}

#[test]
fn the_audit_log_names_the_acting_party_rather_than_only_its_id() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let operator =
            harness::register(&state, "Named Operator", "plat-audit@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;

        // A command run *as* somebody, so `audit.acting_as` names a party: a
        // registration is run by nobody, so its own row names none.
        server
            .post("/api/cmd/create-document")
            .add_header("cookie", format!("rn_session={}", operator.token))
            .add_header("sec-fetch-site", "same-origin")
            .text(r#"{"key":"6f1a0f7e-0000-4000-8000-0000000000a1","title":"Audit","body":"x"}"#)
            .content_type("application/json")
            .await
            .assert_status_ok();

        let rows: Vec<serde_json::Value> = server
            .get("/api/q/platform-audit")
            .add_header("cookie", format!("rn_session={}", operator.token))
            .await
            .json();
        assert!(!rows.is_empty(), "the commands are on the log");

        // Acting as is a party id, and a party has a display name in the same
        // table the id came from — so a row that can name the actor can name
        // the party it acted as, and the console has no reason to print one of
        // them as a name and the other as an id.
        let named = rows
            .iter()
            .filter(|row| !row["acting_as"].is_null())
            .inspect(|row| {
                assert!(
                    row["acting_display"].is_string(),
                    "an acting party with no display name: {row}"
                );
            })
            .count();
        assert!(named > 0, "no audit row named an acting party at all");
        assert!(
            rows.iter()
                .any(|row| row["acting_display"] == "Named Operator"),
            "the operator's own command is not attributed to them by name"
        );
    });
}

// ------------------------------------------------------------- explain ---

/// A deployment with one registration in it, which is all a plan needs.
async fn seeded() -> Local {
    let harness = Local::new();
    harness
        .register("Ronit", "plan-platform@example.test")
        .await
        .expect("registers");
    harness
}

/// Every plan, printed once so a failure says which statement and what it did.
async fn plans() -> Vec<(&'static str, Vec<String>)> {
    let harness = seeded().await;
    let mut all = Vec::new();
    for (name, sql, params) in rn_site::api::query::platform::statements() {
        let plan = harness
            .store()
            .engine()
            .explain(sql, params)
            .unwrap_or_else(|error| panic!("{name}: EXPLAIN QUERY PLAN: {error}"));
        assert!(!plan.is_empty(), "{name} produced no plan");
        all.push((name, plan));
    }
    all
}

/// The statements that legitimately read one table end to end: the four
/// deployment-wide lists, whose whole purpose is "every party", "every
/// registration", "every session", "every resource".
///
/// Even these are not free scans — the rowid order they ask for is the order
/// the primary key is already in, which is why none of them sorts — and every
/// row they join to is a seek. Anything not on this list must seek outright.
const WHOLE_TABLE: &[&str] = &[
    "parties.list",
    "identities.list",
    "sessions.list",
    "resources.list",
    "matches.list",
    "identities.factors_of_page",
];

#[tokio::test]
async fn explain_the_platform_queries_seek_everything_but_their_own_leading_table() {
    for (name, plan) in plans().await {
        let joined = plan.join("\n");
        let scans: Vec<&String> = plan
            .iter()
            .filter(|line| line.contains("SCAN") && !line.contains("USING"))
            .collect();
        if WHOLE_TABLE.contains(&name) {
            assert!(
                scans.len() <= 1,
                "{name} scans more than its own leading table:\n{joined}"
            );
        } else {
            assert!(scans.is_empty(), "{name} scans a table:\n{joined}");
        }
    }
}

#[tokio::test]
async fn explain_no_platform_list_sorts_what_the_primary_key_already_orders() {
    for (name, plan) in plans().await {
        if !WHOLE_TABLE.contains(&name) && name != "audit.tail" {
            continue;
        }
        let joined = plan.join("\n");
        assert!(
            !joined.contains("TEMP B-TREE"),
            "{name} sorts a deployment-wide read:\n{joined}"
        );
    }
}

#[tokio::test]
async fn explain_the_audit_tail_is_a_rowid_range() {
    let joined = plans()
        .await
        .into_iter()
        .find(|(name, _)| *name == "audit.tail")
        .expect("the audit tail is planned")
        .1
        .join("\n");
    assert!(
        joined.contains("SEARCH a USING INTEGER PRIMARY KEY"),
        "the audit tail must be a rowid range:\n{joined}"
    );
}

#[tokio::test]
async fn explain_the_relation_reads_ride_their_own_indexes() {
    let by = |name: &str, index: &str, plans: &[(&'static str, Vec<String>)]| {
        let joined = plans
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("{name} is planned"))
            .1
            .join("\n");
        assert!(
            joined.contains(index),
            "{name} does not ride {index}:\n{joined}"
        );
    };
    let plans = plans().await;
    // Subject-keyed for "what has this party been granted", object-keyed for
    // "who holds anything on this resource" — the two directions, two indexes.
    by("parties.as_subject", "relation_subject_key_idx", &plans);
    by("resources.as_object", "relation_object_idx", &plans);
}
