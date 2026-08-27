//! The organization tier's queries: what they answer, who they answer, and
//! what they cost.
//!
//! Three claims, and they are the three a tenant-scoped read has to make.
//!
//! * **Scope.** Every `org*` query names an organization, and an operator of
//!   one organization gets the uniform decline for every query of another —
//!   the matrix below is that statement, one row per query, run as the admin
//!   of the *other* organization.
//! * **Shape.** The rows carry what the pages render and nothing beside it:
//!   derived public ids, never a rowid, and never a token.
//! * **Cost.** Each statement is asserted under `EXPLAIN QUERY PLAN` to reach
//!   its index. A read that scans is not a read that fails a functional test;
//!   it is one that works on ten rows and cannot be run on ten million.

mod harness;

use axum::http::StatusCode;
use axum_test::TestServer;
use rn_kernel::store::Value;
use rn_kernel::testing::Local;
use rn_kernel::{bind, ids::Id};
use rn_site::api::query::{org_feed, org_rows};
use serde_json::{Value as Json, json};
use tokio::sync::OnceCell;

static NODE: OnceCell<rn_site::AppState> = OnceCell::const_new();

async fn state() -> rn_site::AppState {
    harness::node(&NODE, "127.0.0.1:8195", "127.0.0.1:8295").await
}

/// A signed-in caller over HTTP, as a browser is.
struct Caller<'a> {
    server: &'a TestServer,
    cookie: String,
}

impl<'a> Caller<'a> {
    fn new(server: &'a TestServer, token: &str) -> Self {
        Self {
            server,
            cookie: format!("rn_session={token}"),
        }
    }

    async fn post(&self, name: &str, args: Json) -> axum_test::TestResponse {
        let mut body = args.as_object().cloned().unwrap_or_default();
        body.insert("key".to_owned(), json!(uuid::Uuid::new_v4().to_string()));
        self.server
            .post(&format!("/api/cmd/{name}"))
            .add_header("cookie", self.cookie.clone())
            .add_header("sec-fetch-site", "same-origin")
            .text(Json::Object(body).to_string())
            .content_type("application/json")
            .await
    }

    async fn run(&self, name: &str, args: Json) -> Json {
        let response = self.post(name, args).await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "{name} answered {}: {}",
            response.status_code(),
            response.text()
        );
        response.json::<Json>()["result"].clone()
    }

    async fn get(&self, url: &str) -> axum_test::TestResponse {
        self.server
            .get(url)
            .add_header("cookie", self.cookie.clone())
            .await
    }

    async fn rows(&self, url: &str) -> Vec<Json> {
        let response = self.get(url).await;
        response.assert_status_ok();
        response.json()
    }

    async fn person(&self) -> String {
        let response = self
            .server
            .get("/api/whoami")
            .add_header("cookie", self.cookie.clone())
            .await;
        response.assert_status_ok();
        response.json::<Json>()["person"]["public_id"]
            .as_str()
            .expect("a person")
            .to_owned()
    }
}

/// Every organization query, as a URL against one organization.
fn every_query(org: &str) -> Vec<String> {
    [
        "org",
        "org-members",
        "org-groups",
        "org-documents",
        "org-invitations",
        "org-audit",
    ]
    .iter()
    .map(|name| format!("/api/q/{name}?org={org}"))
    .collect()
}

fn soon() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs() as i64
        + 3600
}

#[test]
fn an_organizations_pages_are_its_own_rows_and_nobody_elses() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let mine = harness::register(&state, "Mine", "org-mine@example.invalid").await;
        let theirs = harness::register(&state, "Theirs", "org-theirs@example.invalid").await;
        let member = harness::register(&state, "Joiner", "org-joiner@example.invalid").await;
        let mine = Caller::new(&server, &mine.token);
        let theirs = Caller::new(&server, &theirs.token);
        let member = Caller::new(&server, &member.token);

        let a = mine
            .run("create-organization", json!({ "display_name": "Alpha" }))
            .await;
        let alpha = a["organization"].as_str().expect("an org id").to_owned();
        let b = theirs
            .run("create-organization", json!({ "display_name": "Beta" }))
            .await;
        let beta = b["organization"].as_str().expect("an org id").to_owned();

        // The overview is the organization, with its counts folded in.
        let rows = mine.rows(&format!("/api/q/org?org={alpha}")).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["display"], "Alpha");
        assert_eq!(rows[0]["members"], 1);
        assert_eq!(rows[0]["my_role"], "owner");
        assert!(
            rows[0]["resource"]
                .as_str()
                .is_some_and(|id| id.starts_with("r_")),
            "the overview must carry what Transfer addresses: {}",
            rows[0]
        );

        // A group, an invitation, a claim, and a document — the whole tier.
        let group = mine
            .run(
                "create-group",
                json!({ "display_name": "Founders", "organization": alpha }),
            )
            .await["group"]
            .as_str()
            .expect("a group id")
            .to_owned();
        let invited = mine
            .run(
                "invite",
                json!({ "group": group, "role": "member", "expires_at": soon() }),
            )
            .await;
        let token = invited["token"].as_str().expect("a token").to_owned();
        member.run("claim-link", json!({ "token": token })).await;
        mine.run(
            "create-document",
            json!({ "title": "Charter", "body": "one", "owner": alpha }),
        )
        .await;

        let groups = mine.rows(&format!("/api/q/org-groups?org={alpha}")).await;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["display"], "Founders");
        assert_eq!(
            groups[0]["members"], 2,
            "the founder owns the group and the claimant joined it"
        );

        let roster = mine
            .rows(&format!("/api/q/org-members?org={alpha}&group={group}"))
            .await;
        assert_eq!(roster.len(), 2);
        let joined = roster
            .iter()
            .find(|row| row["display"] == "Joiner")
            .expect("the claimant is in the group");
        assert_eq!(joined["role"], "member");
        assert_eq!(joined["settable"], true, "an admin may move a member");

        let documents = mine
            .rows(&format!("/api/q/org-documents?org={alpha}"))
            .await;
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0]["title"], "Charter");
        assert_eq!(documents[0]["draft_rev"], 1);

        let invitations = mine
            .rows(&format!("/api/q/org-invitations?org={alpha}"))
            .await;
        assert_eq!(invitations.len(), 1);
        assert_eq!(invitations[0]["claimed_by"], "Joiner");
        assert!(
            !invitations
                .iter()
                .any(|row| row.to_string().contains(&token)),
            "an invitation listing must not carry the token"
        );

        let audit = mine.rows(&format!("/api/q/org-audit?org={alpha}")).await;
        assert!(
            audit.iter().any(|row| row["command"] == "create-group"),
            "the tail must carry what was done to the organization: {audit:?}"
        );
        let filtered = mine
            .rows(&format!("/api/q/org-audit?org={alpha}&command=invite"))
            .await;
        assert!(
            !filtered.is_empty() && filtered.iter().all(|row| row["command"] == "invite"),
            "the command filter must narrow to one command: {filtered:?}"
        );

        // The matrix: an operator of Beta asks for everything of Alpha.
        for url in every_query(&alpha) {
            let response = theirs.get(&url).await;
            assert_eq!(
                response.status_code(),
                StatusCode::FORBIDDEN,
                "{url} answered another organization's operator"
            );
        }
        // And in the other direction, so the row is not passing by accident.
        for url in every_query(&beta) {
            assert_eq!(
                mine.get(&url).await.status_code(),
                StatusCode::FORBIDDEN,
                "{url} answered Alpha's operator"
            );
        }
        // A member who is not an admin holds the rows of neither.
        for url in every_query(&alpha) {
            assert_eq!(
                member.get(&url).await.status_code(),
                StatusCode::FORBIDDEN,
                "{url} answered a plain member"
            );
        }
    });
}

#[test]
fn an_organization_query_without_a_scope_declines_rather_than_guessing() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let who = harness::register(&state, "Scopeless", "org-scopeless@example.invalid").await;
        let caller = Caller::new(&server, &who.token);
        caller
            .run("create-organization", json!({ "display_name": "Gamma" }))
            .await;

        for url in [
            "/api/q/org",
            "/api/q/org?org=",
            "/api/q/org?org=o_AAAAAAAAAAAAAAAAAAAAAA",
            "/api/q/org?org=p_AAAAAAAAAAAAAAAAAAAAAA",
            "/api/q/org-members?org=nonsense",
        ] {
            assert_eq!(
                caller.get(url).await.status_code(),
                StatusCode::FORBIDDEN,
                "{url} answered without a scope it could prove"
            );
        }
    });
}

#[test]
fn the_group_drill_in_refuses_a_group_of_another_organization() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let mine = harness::register(&state, "Owner", "org-drill-a@example.invalid").await;
        let other = harness::register(&state, "Other", "org-drill-b@example.invalid").await;
        let mine = Caller::new(&server, &mine.token);
        let other = Caller::new(&server, &other.token);

        let alpha = mine
            .run("create-organization", json!({ "display_name": "Drill A" }))
            .await["organization"]
            .as_str()
            .expect("an org")
            .to_owned();
        let beta = other
            .run("create-organization", json!({ "display_name": "Drill B" }))
            .await["organization"]
            .as_str()
            .expect("an org")
            .to_owned();
        let theirs = other
            .run(
                "create-group",
                json!({ "display_name": "Theirs", "organization": beta }),
            )
            .await["group"]
            .as_str()
            .expect("a group")
            .to_owned();

        // Ownership is the gate, not membership of the group: a group whose
        // resource is owned by another organization is not this one's.
        for name in ["org-members", "org-contacts", "org-invitations"] {
            let url = format!("/api/q/{name}?org={alpha}&group={theirs}");
            assert_eq!(
                mine.get(&url).await.status_code(),
                StatusCode::FORBIDDEN,
                "{url} drilled into another organization's group"
            );
        }
    });
}

#[test]
fn transferring_an_organization_moves_the_owner_and_leaves_an_admin() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let founder = harness::register(&state, "Founder", "org-hand-a@example.invalid").await;
        let heir = harness::register(&state, "Heir", "org-hand-b@example.invalid").await;
        let founder = Caller::new(&server, &founder.token);
        let heir_caller = Caller::new(&server, &heir.token);
        let heir_person = heir_caller.person().await;

        let created = founder
            .run("create-organization", json!({ "display_name": "Handover" }))
            .await;
        let org = created["organization"].as_str().expect("an org").to_owned();
        let resource = created["resource"].as_str().expect("a resource").to_owned();

        founder
            .run(
                "invite",
                json!({ "group": org, "role": "admin", "expires_at": soon() }),
            )
            .await;
        let token = founder
            .run(
                "invite",
                json!({ "group": org, "role": "admin", "expires_at": soon() }),
            )
            .await["token"]
            .as_str()
            .expect("a token")
            .to_owned();
        heir_caller
            .run("claim-link", json!({ "token": token }))
            .await;

        founder
            .run(
                "transfer",
                json!({ "resource": resource, "to": heir_person }),
            )
            .await;

        // The overview says who owns it, and says so to both of them.
        let after = founder.rows(&format!("/api/q/org?org={org}")).await;
        assert_eq!(after[0]["owner"]["display"], "Heir");
        assert_eq!(
            after[0]["owned_by_me"], false,
            "the previous owner must lose the affordance that transfers it"
        );
        assert_eq!(
            after[0]["my_role"], "admin",
            "an owner who hands it on stays"
        );

        let theirs = heir_caller.rows(&format!("/api/q/org?org={org}")).await;
        assert_eq!(theirs[0]["my_role"], "owner");
        assert_eq!(theirs[0]["owned_by_me"], true);

        // And the roster says the last owner cannot be demoted out of it.
        let roster = heir_caller
            .rows(&format!("/api/q/org-members?org={org}"))
            .await;
        let owner = roster
            .iter()
            .find(|row| row["role"] == "owner")
            .expect("an owner");
        assert_eq!(owner["last_owner"], true);
        assert_eq!(owner["settable"], false);
    });
}

// ------------------------------------------------------------- the plans ---
//
// One test per statement, over a real schema with a row in it, asserting the
// index each one reaches. The harness is the kernel's in-process SQLite,
// because `EXPLAIN QUERY PLAN` is a SQLite answer and asking it through raft
// would be asking the same planner one hop further away.

/// Assert a plan reaches `index` and scans nothing without one.
#[track_caller]
fn rides(plan: &[String], index: &str) {
    let joined = plan.join("\n");
    assert!(
        joined.contains(index),
        "the plan does not use {index}:\n{joined}"
    );
    for line in plan {
        // A `json_each` is always a scan and is always the parameter list the
        // caller passed in — a handful of container ids, never a table.
        assert!(
            !line.contains("SCAN") || line.contains("USING") || line.contains("VIRTUAL TABLE"),
            "a full scan crept into a scoped read:\n{joined}"
        );
    }
}

async fn plan(harness: &Local, sql: &str, params: Vec<Value>) -> Vec<String> {
    harness
        .store()
        .engine()
        .explain(sql, params)
        .expect("EXPLAIN QUERY PLAN runs")
}

/// A harness with one registration in it, so the planner has statistics that
/// are not the empty ones.
async fn seeded() -> Local {
    let harness = Local::new();
    harness
        .register("Ronit", "org-plan@example.test")
        .await
        .expect("registers");
    harness
}

#[tokio::test]
async fn explain_the_overview_reaches_the_party_by_its_row() {
    let harness = seeded().await;
    let plan = plan(&harness, org_rows::OVERVIEW, bind![1_i64]).await;
    rides(&plan, "party");
}

#[tokio::test]
async fn explain_the_member_count_rides_the_membership_primary_key() {
    let harness = seeded().await;
    let plan = plan(&harness, org_rows::MEMBER_COUNT, bind![1_i64]).await;
    rides(&plan, "membership");
}

#[tokio::test]
async fn explain_the_owned_counts_ride_the_owner_index() {
    let harness = seeded().await;
    let plan = plan(&harness, org_rows::OWNED_COUNT, bind![1_i64, "document"]).await;
    rides(&plan, "resource_owner_idx");
}

#[tokio::test]
async fn explain_the_group_listing_rides_the_owner_index() {
    let harness = seeded().await;
    let plan = plan(&harness, org_rows::GROUPS, bind![1_i64]).await;
    rides(&plan, "resource_owner_idx");
}

#[tokio::test]
async fn explain_contact_scoping_rides_the_subject_key_index() {
    let harness = seeded().await;
    let plan = plan(&harness, org_rows::CONTACTS, bind!["group:1"]).await;
    rides(&plan, "relation_subject_key_idx");
}

#[tokio::test]
async fn explain_the_document_listing_rides_the_owner_index() {
    let harness = seeded().await;
    let plan = plan(&harness, org_feed::DOCUMENTS, bind![1_i64, 200_i64]).await;
    rides(&plan, "resource_owner_idx");
}

#[tokio::test]
async fn explain_a_pages_grants_ride_the_object_index() {
    let harness = seeded().await;
    let plan = plan(&harness, org_feed::GRANTS, bind!["[1,2,3]"]).await;
    // Either index on `(object_kind, object_id)` is a seek; the planner
    // takes the covering one when it can, which is strictly better.
    rides(&plan, "relation_unique_idx");
}

#[tokio::test]
async fn explain_invitations_ride_the_object_index() {
    let harness = seeded().await;
    let plan = plan(
        &harness,
        org_feed::INVITATIONS,
        bind!["[1,2,3]", "organization"],
    )
    .await;
    rides(&plan, "relation_object_idx");
}

#[tokio::test]
async fn explain_the_audit_tail_is_a_forward_range_over_the_offset() {
    let harness = seeded().await;
    let plan = plan(
        &harness,
        org_feed::AUDIT,
        bind![0_i64, "", "[1,2,3]", 100_i64],
    )
    .await;
    let joined = plan.join("\n");
    // `a.id > ?` over the rowid: the change-feed offset *is* the primary key,
    // so a page is a seek from where the reader got to. The container test is
    // a filter inside that range and never the way the range is found.
    assert!(
        joined.contains("SEARCH a USING INTEGER PRIMARY KEY (rowid>?)"),
        "the audit tail must be a rowid range:\n{joined}"
    );
    assert!(
        !joined.contains("TEMP B-TREE"),
        "the tail must not sort what the primary key already orders:\n{joined}"
    );
}

/// The ids the plans above bind are rowids, and a rowid never leaves the
/// process — so this test says out loud that these constants are only ever
/// reached through [`rn_site::api::query::org::scope`], which takes a public
/// id and proves a membership before any of them runs.
#[test]
fn a_scoped_statement_is_bound_with_a_rowid_and_reached_through_a_public_one() {
    assert!(org_rows::OVERVIEW.contains("$1"));
    assert!(!org_rows::OVERVIEW.contains("public"));
    let _: fn(Id<rn_kernel::ids::Person>) -> i64 = Id::get;
}
