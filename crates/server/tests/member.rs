//! The member tier's queries: the plan each statement rides, and the rows each
//! one refuses to carry.
//!
//! Two claims, and they are different claims. The `explain_` half is the "no
//! full scan" gate applied to the statements this leg added: a missing index
//! does not fail a functional test — it makes one slightly slower on ten rows
//! and unusable on ten million — so what is asserted is the plan. The rest is
//! the authorisation half: every query here is "mine", and the one that takes
//! a row declines somebody else's rather than answering about it.

mod harness;

use axum::http::StatusCode;
use rn_kernel::bind;
use rn_kernel::store::Value;
use rn_kernel::testing::Local;
use rn_site::api::query::{ALL, Scope, member_documents, member_groups, member_matches};
use serde_json::{Value as Json, json};
use tokio::sync::OnceCell;

static NODE: OnceCell<rn_site::AppState> = OnceCell::const_new();

async fn state() -> rn_site::AppState {
    harness::node(&NODE, "127.0.0.1:8197", "127.0.0.1:8297").await
}

/// Assert a plan reaches the named index and scans nothing but its own
/// arguments. `json_each` is a list the query was handed, not a table it went
/// looking through, so a virtual-table scan is not a scan of anything.
#[track_caller]
fn rides(plan: &[String], index: &str) {
    let joined = plan.join("\n");
    assert!(
        joined.contains(index),
        "the plan does not use {index}:\n{joined}"
    );
    for line in plan {
        assert!(
            !line.contains("SCAN") || line.contains("VIRTUAL TABLE"),
            "a full scan crept into a member query:\n{joined}"
        );
    }
}

fn plan(harness: &Local, sql: &str, params: Vec<Value>) -> Vec<String> {
    harness
        .store()
        .engine()
        .explain(sql, params)
        .expect("EXPLAIN QUERY PLAN runs")
}

const KEYS: &str = r#"["public:0","authenticated:0","person:1","group:7"]"#;

#[tokio::test]
async fn explain_my_groups_ride_my_own_membership() {
    let harness = Local::new();
    let plan = plan(&harness, member_groups::GROUPS_SQL, bind![1i64]);
    rides(&plan, "membership_party_idx");
    rides(&plan, "sqlite_autoindex_party_resource_1");
}

#[tokio::test]
async fn explain_a_roster_is_reached_through_my_own_membership() {
    let harness = Local::new();
    let plan = plan(&harness, member_groups::MEMBERS_SQL, bind![1i64]);
    // My membership first, then the group's roster by the membership key: a
    // group I am not in is never a row this statement reaches.
    rides(&plan, "membership_party_idx");
    rides(&plan, "sqlite_autoindex_membership_1");
}

#[tokio::test]
async fn explain_my_invitations_seek_the_minter_rather_than_filtering_for_them() {
    let harness = Local::new();
    let plan = plan(&harness, member_groups::INVITATIONS_SQL, bind![1i64]);
    // The grant is the driving row and the link hangs off its rowid. The seek
    // is on `granted_by` — the record of who minted an invitation — so this
    // reaches one person's invitations rather than every link grant in the
    // deployment (migration 4). `subject_kind` narrows within the seek.
    rides(&plan, "relation_granted_by_idx");
    rides(&plan, "INTEGER PRIMARY KEY");
}

#[tokio::test]
async fn explain_the_document_list_seeks_both_ways_a_document_becomes_visible() {
    let harness = Local::new();
    let plan = plan(
        &harness,
        member_documents::DOCUMENTS_SQL,
        bind![KEYS, "[1]", 200i64],
    );
    rides(&plan, "resource_kind_created_idx");
    rides(&plan, "relation_subject_key_idx");
    rides(&plan, "resource_owner_idx");
}

#[tokio::test]
async fn explain_one_document_is_a_primary_key_seek() {
    let harness = Local::new();
    let plan = plan(&harness, member_documents::DOCUMENT_SQL, bind![1i64]);
    // Three rowid seeks: the resource row, its document half, its owner.
    assert_eq!(
        plan.iter()
            .filter(|line| line.contains("INTEGER PRIMARY KEY"))
            .count(),
        3,
        "{plan:?}"
    );
    rides(&plan, "INTEGER PRIMARY KEY");
}

#[tokio::test]
async fn explain_what_i_hold_is_bounded_by_the_page_it_is_read_for() {
    let harness = Local::new();
    let plan = plan(&harness, member_documents::HELD_SQL, bind!["[1,2]", KEYS]);
    // The page drives and the seek carries the document id: every column of
    // `relation_subject_key_idx` is bound, so this is an index-only lookup
    // per (listed document, subject) rather than a walk of every grant the
    // principal holds anywhere. What it used to be — `subject_key` bound and
    // nothing else — returned 10 004 rows to render a page of 200 (finding 1,
    // docs/perf/2026-08-27.md), and the shape of that is a seek that stops at
    // `subject_key=?`.
    rides(&plan, "relation_subject_key_idx");
    assert!(
        plan.join("\n")
            .contains("(subject_key=? AND object_kind=? AND object_id=?)"),
        "the held read must be bounded by the page it is read for:\n{}",
        plan.join("\n")
    );
}

#[tokio::test]
async fn explain_the_share_panel_is_keyed_on_the_object() {
    let harness = Local::new();
    let plan = plan(&harness, member_documents::SHARES_SQL, bind!["[1,2]"]);
    rides(&plan, "relation_object_idx");
}

#[tokio::test]
async fn explain_the_match_queue_seeks_both_sides_of_a_pair() {
    let harness = Local::new();
    let plan = plan(&harness, member_matches::CANDIDATES_SQL, bind!["[1,2]"]);
    rides(&plan, "match_candidate_a_idx");
    rides(&plan, "match_candidate_b_idx");
}

#[tokio::test]
async fn explain_my_identities_ride_the_person_index() {
    let harness = Local::new();
    let mine = plan(&harness, member_matches::MY_IDENTITIES_SQL, bind![1i64]);
    rides(&mine, "identity_person_idx");
    let names = plan(&harness, member_matches::NAMES_SQL, bind!["[1,2]"]);
    rides(&names, "INTEGER PRIMARY KEY");
}

// ----------------------------------------------------------- the matrix ---

#[test]
fn every_member_query_answers_the_member_and_nobody_else() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let mine = harness::register(&state, "Mine", "member-mine@example.invalid").await;
        let cookie = format!("rn_session={}", mine.token);

        for named in ALL {
            let query = named.as_str();
            let response = server
                .get(&format!("/api/q/{query}"))
                .add_header("cookie", cookie.clone())
                .await;
            // The member tier's own queries answer; `document` takes a row,
            // and asking for none is asking about nothing. The other tiers'
            // queries decline a plain member with the same uniform refusal
            // they give a stranger.
            let expected = match named.scope() {
                Scope::Identity | Scope::Set => StatusCode::OK,
                Scope::Once | Scope::Org | Scope::Platform => StatusCode::FORBIDDEN,
            };
            assert_eq!(
                response.status_code(),
                expected,
                "/api/q/{query} answered {} — {}",
                response.status_code(),
                response.text()
            );

            // Anonymous is refused everywhere, uniformly.
            let anonymous = server.get(&format!("/api/q/{query}")).await;
            assert_eq!(
                anonymous.status_code(),
                StatusCode::FORBIDDEN,
                "/api/q/{query} answered a stranger"
            );
        }
    });
}

#[test]
fn a_document_is_readable_by_the_people_it_was_shared_with_and_declined_to_the_rest() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let author = harness::register(&state, "Author", "member-author@example.invalid").await;
        let reader = harness::register(&state, "Reader", "member-reader@example.invalid").await;
        let stranger = harness::register(&state, "Nobody", "member-stranger@example.invalid").await;

        let mine = format!("rn_session={}", author.token);
        let theirs = format!("rn_session={}", reader.token);
        let outside = format!("rn_session={}", stranger.token);

        let created = command(
            &server,
            &mine,
            "create-document",
            json!({"title": "Kernel report", "body": "Five rules"}),
        )
        .await;
        let document = created["result"]["document"]
            .as_str()
            .expect("a document id")
            .to_owned();

        // The author's own list carries it; nobody else's does.
        assert_eq!(rows(&server, &mine, "documents").await.len(), 1);
        assert!(rows(&server, &theirs, "documents").await.is_empty());

        // And the row itself is declined to everyone it was not shared with —
        // the same bytes whether the id is a stranger's or a fiction.
        for cookie in [&theirs, &outside] {
            let response = server
                .get(&format!("/api/q/document?id={document}"))
                .add_header("cookie", cookie.clone())
                .await;
            assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
        }

        let person = whoami(&server, &theirs).await["person"]["public_id"]
            .as_str()
            .expect("a person id")
            .to_owned();
        command(
            &server,
            &mine,
            "share",
            json!({"resource": document, "subject": person, "relation": "viewer"}),
        )
        .await;

        // Now it is in their list, and readable — and still not the
        // stranger's.
        assert_eq!(rows(&server, &theirs, "documents").await.len(), 1);
        let read = server
            .get(&format!("/api/q/document?id={document}"))
            .add_header("cookie", theirs.clone())
            .await;
        read.assert_status_ok();
        let body: Vec<Json> = read.json();
        assert_eq!(body[0]["body"], "Five rules");
        assert_eq!(body[0]["may_edit"], false);
        assert_eq!(
            server
                .get(&format!("/api/q/document?id={document}"))
                .add_header("cookie", outside.clone())
                .await
                .status_code(),
            StatusCode::FORBIDDEN
        );

        // The share panel names the grant, by public id and never by a rowid.
        let shares = rows(&server, &mine, "shares").await;
        assert!(
            shares
                .iter()
                .any(|row| row["subject"] == person.as_str() && row["relation"] == "viewer"),
            "the share is not in the panel: {shares:?}"
        );
    });
}

#[test]
fn a_group_carries_its_roster_and_its_invitations_to_the_people_in_it() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let founder = harness::register(&state, "Founder", "member-founder@example.invalid").await;
        let joiner = harness::register(&state, "Joiner", "member-joiner@example.invalid").await;
        let mine = format!("rn_session={}", founder.token);
        let theirs = format!("rn_session={}", joiner.token);

        let created = command(
            &server,
            &mine,
            "create-group",
            json!({"display_name": "Founders"}),
        )
        .await;
        let group = created["result"]["group"]
            .as_str()
            .expect("a group")
            .to_owned();

        assert_eq!(rows(&server, &mine, "groups").await.len(), 1);
        assert!(rows(&server, &theirs, "groups").await.is_empty());
        assert_eq!(rows(&server, &mine, "group-members").await.len(), 1);
        // A group somebody else is in is not a roster this reader can reach.
        assert!(rows(&server, &theirs, "group-members").await.is_empty());

        let invited = command(
            &server,
            &mine,
            "invite",
            json!({"group": group, "role": "member", "expires_at": soon()}),
        )
        .await;
        let token = invited["result"]["token"]
            .as_str()
            .expect("a token")
            .to_owned();

        // The invitation is the minter's to see, and only the minter's.
        let invitations = rows(&server, &mine, "invitations").await;
        assert_eq!(invitations.len(), 1);
        assert_eq!(invitations[0]["claimed_by"], Json::Null);
        assert!(
            !server
                .get("/api/q/invitations")
                .add_header("cookie", mine.clone())
                .await
                .text()
                .contains(&token),
            "an invitation list handed back the secret that opens it"
        );
        assert!(rows(&server, &theirs, "invitations").await.is_empty());

        command(&server, &theirs, "claim-link", json!({"token": token})).await;

        // Both sides now see the roster, and the minter sees who claimed.
        assert_eq!(rows(&server, &mine, "group-members").await.len(), 2);
        assert_eq!(rows(&server, &theirs, "group-members").await.len(), 2);
        assert_eq!(
            rows(&server, &mine, "invitations").await[0]["claimed_by"],
            "Joiner"
        );
    });
}

#[test]
fn a_set_query_is_subscribable_and_the_one_that_takes_a_row_is_not() {
    for query in ALL {
        let subscribable = query.scope() != Scope::Once;
        assert_eq!(
            subscribable,
            query.as_str() != "document",
            "{} is subscribable: {subscribable}",
            query.as_str()
        );
    }
}

#[test]
fn a_key_that_left_a_shared_set_is_reported_as_having_left_it() {
    use rn_site::api::query::{Row, set_ops};

    let mut sent = std::collections::BTreeMap::new();
    let ops = set_ops(
        &mut sent,
        vec![
            Row {
                key: "a".to_owned(),
                value: json!({"n": 1}),
            },
            Row {
                key: "b".to_owned(),
                value: json!({"n": 2}),
            },
        ],
    );
    assert_eq!(ops.len(), 2, "the first read of a set is all of it");

    let ops = set_ops(
        &mut sent,
        vec![
            Row {
                key: "a".to_owned(),
                value: json!({"n": 1}),
            },
            Row {
                key: "c".to_owned(),
                value: json!({"n": 3}),
            },
        ],
    );
    // `a` did not move and is not sent; `c` arrived; `b` is gone.
    assert_eq!(ops.len(), 2, "{ops:?}");
    assert!(format!("{ops:?}").contains("Put"));
    assert!(format!("{ops:?}").contains(r#"Del { key: "b" }"#));
}

// --------------------------------------------------------------- helpers ---

async fn rows(server: &axum_test::TestServer, cookie: &str, query: &str) -> Vec<Json> {
    let response = server
        .get(&format!("/api/q/{query}"))
        .add_header("cookie", cookie.to_owned())
        .await;
    response.assert_status_ok();
    response.json()
}

async fn whoami(server: &axum_test::TestServer, cookie: &str) -> Json {
    server
        .get("/api/whoami")
        .add_header("cookie", cookie.to_owned())
        .await
        .json()
}

async fn command(server: &axum_test::TestServer, cookie: &str, name: &str, args: Json) -> Json {
    let mut body = args.as_object().cloned().unwrap_or_default();
    body.insert("key".to_owned(), json!(uuid::Uuid::new_v4().to_string()));
    let response = server
        .post(&format!("/api/cmd/{name}"))
        .add_header("cookie", cookie.to_owned())
        .add_header("sec-fetch-site", "same-origin")
        .text(Json::Object(body).to_string())
        .content_type("application/json")
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "{name} answered {} — {}",
        response.status_code(),
        response.text()
    );
    response.json()
}

fn soon() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs() as i64
        + 3600
}
