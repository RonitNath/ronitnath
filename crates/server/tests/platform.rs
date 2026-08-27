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
use rn_kernel::store::Reads as _;
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
    assert_eq!(names.len(), 19, "the platform surface is nineteen queries");
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
    // One row per voter, and the deployment has three at most. The order it
    // asks for is the primary key's, so there is no sort either.
    "nodes.list",
    // The four registries an operator opens whole: every signing key, every
    // relying party, every outstanding invitation, every consent. Each asks
    // for the order its primary key is already in, so none of them sorts, and
    // every row they join or count through is a seek.
    "keys.list",
    "clients.list",
    "links.list",
    "consents.list",
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

// -------------------------------------------------- what the pages say ---

/// The person page prints every factor a human has proven, and an operator
/// console that printed the addresses would be the enumeration endpoint the
/// whole identity model exists not to have. One screenshot onto a support
/// ticket is all it takes.
#[test]
fn the_person_a_page_renders_carries_no_address_at_all() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let operator = harness::register(&state, "Reader", "plat-mask-op@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;
        // Somebody whose address is distinctive enough that finding any part
        // of it in the answer is finding the address.
        let subject =
            harness::register(&state, "Masked", "plat-mask-subject@example.invalid").await;

        let id = {
            let response = server
                .get("/api/q/platform-parties")
                .add_header("cookie", format!("rn_session={}", operator.token))
                .await;
            response.assert_status_ok();
            let rows: Vec<serde_json::Value> = response.json();
            rows.iter()
                .find(|row| row["display"] == "Masked")
                .expect("the party is in the list")["public_id"]
                .as_str()
                .expect("a public id")
                .to_owned()
        };

        let response = server
            .get(&format!("/api/q/platform-party?subject={id}"))
            .add_header("cookie", format!("rn_session={}", operator.token))
            .await;
        response.assert_status_ok();
        let body = response.text();

        // The claim, exactly: no `@`-bearing address survives. Every `@` in
        // the answer is a mask's trailing one, which carries the first and
        // last character of a local part and nothing else — enough to
        // recognise a row on a support call, not enough to write to it.
        assert!(
            !body.contains("example.invalid"),
            "a domain reached the answer: {body}"
        );
        for (index, _) in body.match_indices('@') {
            // A mask is `…@` or `X…Y@`, so the `@` is preceded either by the
            // ellipsis itself or by one character that follows it.
            let before = &body[..index];
            let masked = before.ends_with('\u{2026}')
                || before
                    .char_indices()
                    .next_back()
                    .is_some_and(|(last, _)| before[..last].ends_with('\u{2026}'));
            assert!(
                masked,
                "an unmasked address reached the answer at byte {index}: {body}"
            );
        }

        // And what it *does* carry is the mask, on a factor that says which
        // kind it is and whether it has been proven.
        let rows: Vec<serde_json::Value> = serde_json::from_str(&body).expect("an array");
        let person = rows.first().expect("one party");
        let factors = person["factors"].as_array().expect("a factor list");
        let email = factors
            .iter()
            .find(|factor| factor["kind"] == "email")
            .expect("the registration proved an address");
        assert_eq!(email["hint"], "p\u{2026}t@");
        // A password has no half worth showing, so it shows none.
        let password = factors
            .iter()
            .find(|factor| factor["kind"] == "password")
            .expect("the registration set a password");
        assert!(password["hint"].is_null(), "{password}");
        assert!(
            subject.person.get() > 0,
            "the subject resolved to a person row"
        );
    });
}

/// The one box: three exact seeks, a miss that is empty rather than a decline,
/// and no prefix scan over a name.
#[test]
fn the_search_box_answers_exactly_or_empty() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let operator = harness::register(&state, "Finder", "plat-find-op@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;
        let cookie = format!("rn_session={}", operator.token);
        harness::register(&state, "Findable", "plat-findable@example.invalid").await;

        let ask = async |q: &str| -> Vec<serde_json::Value> {
            let response = server
                .get(&format!("/api/q/platform-find?q={q}"))
                .add_header("cookie", cookie.clone())
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::OK,
                "a search is never a decline: {}",
                response.text()
            );
            response.json()
        };

        // The exact address finds the person behind the registration.
        let hit = ask("plat-findable@example.invalid").await;
        assert_eq!(hit.len(), 1, "{hit:?}");
        assert_eq!(hit[0]["display"], "Findable");
        assert_eq!(hit[0]["via"], "email");

        // The exact public id finds the same row by another road.
        let id = hit[0]["public_id"].as_str().expect("an id").to_owned();
        let by_id = ask(&id).await;
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0]["public_id"], serde_json::json!(id));
        assert_eq!(by_id[0]["via"], "id");

        // A prefix of the display name is not a search: this box seeks and
        // never scans, and saying so by finding nothing is the honest answer.
        assert!(ask("Find").await.is_empty());
        // Forty characters of junk: not a handle, not an address, not an id.
        assert!(ask(&"9x_".repeat(14)).await.is_empty());
        // And a miss is empty rather than a decline — an operator searching is
        // not being probed.
        assert!(ask("nobody@example.invalid").await.is_empty());
    });
}

/// The rollout verdict is counted from the rows, so it can only say what it
/// counted — and a node that has stopped reporting says so rather than
/// leaving the version it left on standing for the deployment's.
#[test]
fn the_rollout_verdict_names_the_versions_it_counted() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let operator = harness::register(&state, "Watcher", "plat-nodes@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;
        let cookie = format!("rn_session={}", operator.token);

        let seed = async |node_id: i64, version: &str, age: i64| {
            let now = state.store.reads().now();
            state
                .store
                .execute(
                    "INSERT INTO node_report \
                     (node_id, node, version, raft_role, feed_head, reported_at) \
                     VALUES ($1, $2, $3, 'follower', 40, $4) \
                     ON CONFLICT (node_id) DO UPDATE SET \
                       version = excluded.version, reported_at = excluded.reported_at",
                    bind![node_id, format!("node{node_id}"), version, now - age],
                )
                .await
                .expect("a report row");
        };
        let nodes = async || -> Vec<serde_json::Value> {
            server
                .get("/api/q/platform-nodes")
                .add_header("cookie", cookie.clone())
                .await
                .json()
        };

        // Three nodes, two versions, all of them speaking.
        seed(1, "9f21ac3", 0).await;
        seed(2, "9f21ac3", 2).await;
        seed(3, "8b0e114", 3).await;
        let rows = nodes().await;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["rollout"], "in progress");
        let versions = rows[0]["versions"].as_array().expect("the versions");
        assert_eq!(versions.len(), 2, "both versions are named: {versions:?}");
        assert!(versions.contains(&serde_json::json!("9f21ac3")));
        assert!(versions.contains(&serde_json::json!("8b0e114")));
        assert!(rows.iter().all(|row| row["reporting"] == true));

        // A third version is nobody being able to say what is running.
        seed(3, "3c5d992", 3).await;
        seed(2, "8b0e114", 2).await;
        assert_eq!(nodes().await[0]["rollout"], "divergent");

        // And the node that stopped speaking is *not reporting* rather than
        // holding the deployment in a rollout with the version it left on.
        seed(3, "3c5d992", 600).await;
        seed(2, "9f21ac3", 2).await;
        let rows = nodes().await;
        assert_eq!(rows[2]["reporting"], false, "{:?}", rows[2]);
        assert!(rows[2]["reported_ago"].as_i64().expect("an age") >= 600);
        assert_eq!(
            rows[0]["rollout"], "settled",
            "a departed node's version is not the deployment's: {rows:?}"
        );
        assert_eq!(rows[0]["reporting"], true);
    });
}

/// Every node writes its own row and nobody else's, from the observation lane.
#[test]
fn a_node_writes_down_what_it_is() {
    harness::run(async {
        let state = state().await;
        state
            .store
            .execute("DELETE FROM node_report", bind![])
            .await
            .expect("an empty table");
        assert!(rn_site::observe::report(&state).await, "the row is written");
        // Twice is one row: the upsert is on the node id, so a restart reuses
        // the row rather than growing the table.
        assert!(rn_site::observe::report(&state).await);
        let rows = state
            .store
            .reads()
            .query::<rn_kernel::store::Count>("SELECT count(*) AS n FROM node_report", bind![])
            .await
            .expect("the count");
        assert_eq!(rows[0].0, 1);
    });
}
