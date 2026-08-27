//! Every command the contract declares, run the way a browser runs it.
//!
//! `api.rs` proves the shape of a reply on one command; this proves the
//! *table*. Each of the contract's names is posted to `/api/cmd/<name>` over
//! a real session, and each reply is held to the same two claims: the offset
//! advances, so the write is somewhere a subscription can wait for, and the
//! result carries the public ids that command created — derived, prefixed, and
//! never a rowid.
//!
//! The commands are not run in isolation, because most of them cannot be: you
//! cannot set a role in a group nobody has joined. So they run as the three
//! journeys they belong to — an organization founded and grown, a document
//! written and handed on, two registrations proven to be one human and taken
//! apart again — and the fixtures each step needs are the previous step's
//! reply, which is the strongest available statement that the ids on this wire
//! are the ones the next command accepts.

mod harness;

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::{Value, json};
use tokio::sync::OnceCell;

static NODE: OnceCell<rn_site::AppState> = OnceCell::const_new();

async fn state() -> rn_site::AppState {
    harness::node(&NODE, "127.0.0.1:8199", "127.0.0.1:8299").await
}

/// A signed-in caller, and the last offset it saw.
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

    /// Post a command and hand back the whole response.
    async fn post(&self, name: &str, args: Value) -> axum_test::TestResponse {
        let mut body = args.as_object().cloned().unwrap_or_default();
        body.insert("key".to_owned(), json!(uuid::Uuid::new_v4().to_string()));
        self.server
            .post(&format!("/api/cmd/{name}"))
            .add_header("cookie", self.cookie.clone())
            .add_header("sec-fetch-site", "same-origin")
            .text(Value::Object(body).to_string())
            .content_type("application/json")
            .await
    }

    /// Run a command that must succeed, and hold its reply to the contract:
    /// the offset moved forward, and the result is an object of public ids.
    async fn run(&self, watermark: &mut u64, name: &str, args: Value) -> Value {
        let response = self.post(name, args).await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "{name} answered {} — body {}",
            response.status_code(),
            response.text()
        );
        let reply: Value = response.json();
        let offset = reply["offset"].as_u64().expect("an offset");
        assert!(
            offset > *watermark,
            "{name} landed at {offset}, which is not past {watermark}"
        );
        *watermark = offset;
        let result = reply["result"].clone();
        assert!(result.is_object(), "{name} returned {result}");
        result
    }

    /// This caller's own person, as the chrome names it.
    async fn person(&self) -> String {
        id_of(&self.whoami().await["person"], "public_id", "p_")
    }

    /// The registration this caller signed in with.
    async fn identity(&self) -> String {
        id_of(&self.whoami().await["identity"], "public_id", "i_")
    }

    /// What the chrome says this caller is.
    async fn whoami(&self) -> Value {
        let response = self
            .server
            .get("/api/whoami")
            .add_header("cookie", self.cookie.clone())
            .await;
        response.assert_status_ok();
        response.json()
    }
}

/// Assert a public id of the expected kind, and hand it back.
#[track_caller]
fn id_of(result: &Value, field: &str, prefix: &str) -> String {
    let id = result[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not an id in {result}"));
    assert!(id.starts_with(prefix), "{field} is {id}, wanted {prefix}…");
    assert_eq!(id.len(), 24, "{field} is not a derived id: {id}");
    id.to_owned()
}

/// An hour from now, which is inside the invitation cap.
fn soon() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs() as i64
        + 3600
}

#[test]
fn an_organization_is_founded_grown_and_left_over_http() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let founder = harness::register(&state, "Founder", "cmd-founder@example.invalid").await;
        let joiner = harness::register(&state, "Joiner", "cmd-joiner@example.invalid").await;
        let founder = Caller::new(&server, &founder.token);
        let joiner = Caller::new(&server, &joiner.token);
        let joiner_person = joiner.person().await;
        let mut at = 0;

        let organization = founder
            .run(
                &mut at,
                "create-organization",
                json!({ "display_name": "Isoastra" }),
            )
            .await;
        let org = id_of(&organization, "organization", "o_");
        id_of(&organization, "resource", "r_");
        id_of(&organization, "owner", "p_");

        let created = founder
            .run(
                &mut at,
                "create-group",
                json!({ "display_name": "Founders", "organization": org }),
            )
            .await;
        let group = id_of(&created, "group", "g_");
        id_of(&created, "resource", "r_");
        // A group inside an organization is owned by the organization, and the
        // reply says so with an organization's id — not with the person id its
        // party row would produce under the wrong tag.
        assert_eq!(id_of(&created, "owner", "o_"), org);

        // Two invitations: one claimed through the command, one through the
        // claim page, because the page is a different route into the same
        // command and both have to land a membership.
        let invited = founder
            .run(
                &mut at,
                "invite",
                json!({ "group": group, "role": "member", "expires_at": soon() }),
            )
            .await;
        assert_eq!(id_of(&invited, "container", "g_"), group);
        let token = invited["token"].as_str().expect("a link token").to_owned();
        id_of(&invited, "link", "l_");

        let claimed = joiner
            .run(&mut at, "claim-link", json!({ "token": token }))
            .await;
        assert_eq!(id_of(&claimed, "container", "g_"), group);
        assert_eq!(id_of(&claimed, "party", "p_"), joiner_person);
        id_of(&claimed, "identity", "i_");

        // The same token twice is a token that no longer works.
        let again = joiner.post("claim-link", json!({ "token": token })).await;
        assert_eq!(again.status_code(), StatusCode::FORBIDDEN);

        let role = founder
            .run(
                &mut at,
                "set-role",
                json!({ "group": group, "party": joiner_person, "role": "admin" }),
            )
            .await;
        assert_eq!(id_of(&role, "container", "g_"), group);
        assert_eq!(id_of(&role, "party", "p_"), joiner_person);

        // A third invitation, minted and then thought better of. The reply
        // names the row; the token that would open it is never asked for.
        let regretted = founder
            .run(
                &mut at,
                "invite",
                json!({ "group": group, "role": "member", "expires_at": soon() }),
            )
            .await;
        let link = id_of(&regretted, "link", "l_");
        let withdrawn = founder
            .run(&mut at, "revoke-link", json!({ "link": link }))
            .await;
        assert_eq!(id_of(&withdrawn, "link", "l_"), link);
        assert_eq!(id_of(&withdrawn, "container", "g_"), group);
        let stale = founder.post("revoke-link", json!({ "link": link })).await;
        assert_eq!(stale.status_code(), StatusCode::FORBIDDEN);

        let left = joiner
            .run(&mut at, "leave", json!({ "group": group }))
            .await;
        assert_eq!(id_of(&left, "container", "g_"), group);
        assert_eq!(id_of(&left, "party", "p_"), joiner_person);

        // And the claim *page* runs the same command, over a bearer token the
        // reader holds and a cookie that says who they are.
        let second = founder
            .run(
                &mut at,
                "invite",
                json!({ "group": group, "role": "member", "expires_at": soon() }),
            )
            .await;
        let token = second["token"].as_str().expect("a link token");
        let page = server.get(&format!("/links/{token}")).await;
        page.assert_status_ok();
        let response = server
            .post(&format!("/links/{token}/claim"))
            .add_header("cookie", joiner.cookie.clone())
            .add_header("sec-fetch-site", "same-origin")
            .text("")
            .content_type("application/x-www-form-urlencoded")
            .await;
        assert_eq!(response.status_code(), StatusCode::SEE_OTHER);
        // Claimed, so the link is now indistinguishable from one that never
        // existed — which is only true if the command actually ran.
        assert_eq!(
            server.get(&format!("/links/{token}")).await.status_code(),
            StatusCode::NOT_FOUND
        );
    });
}

#[test]
fn a_document_is_written_published_shared_and_handed_on() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let author = harness::register(&state, "Author", "cmd-author@example.invalid").await;
        let reader = harness::register(&state, "Reader", "cmd-reader@example.invalid").await;
        let author = Caller::new(&server, &author.token);
        let reader_person = Caller::new(&server, &reader.token).person().await;
        let mut at = 0;

        let created = author
            .run(
                &mut at,
                "create-document",
                json!({ "title": "Kernel report", "body": "Five rules" }),
            )
            .await;
        let document = id_of(&created, "document", "r_");
        id_of(&created, "owner", "p_");

        // A draft starts at revision one, and an edit reports the revision it
        // produced rather than the one it was sent.
        let edited = author
            .run(
                &mut at,
                "edit-document",
                json!({ "document": document, "expected_rev": 1, "title": "Kernel report, revised" }),
            )
            .await;
        assert_eq!(id_of(&edited, "document", "r_"), document);
        assert_eq!(edited["rev"], 2);

        // The same revision twice is the lost update the guard exists for.
        let stale = author
            .post(
                "edit-document",
                json!({ "document": document, "expected_rev": 1, "body": "overwritten" }),
            )
            .await;
        assert_eq!(stale.status_code(), StatusCode::FORBIDDEN);

        let published = author
            .run(&mut at, "publish-document", json!({ "document": document }))
            .await;
        assert_eq!(published["rev"], 2);

        let shared = author
            .run(
                &mut at,
                "share",
                json!({ "resource": document, "subject": reader_person, "relation": "viewer" }),
            )
            .await;
        assert_eq!(shared["object_kind"], "document");
        assert_eq!(shared["relation"], "viewer");
        assert_eq!(id_of(&shared, "object", "r_"), document);

        let revoked = author
            .run(
                &mut at,
                "revoke",
                json!({ "resource": document, "subject": reader_person, "relation": "viewer" }),
            )
            .await;
        assert_eq!(revoked["object_kind"], "document");
        assert_eq!(id_of(&revoked, "object", "r_"), document);

        // Ownership moves through `Transfer` and nowhere else.
        let organization = author
            .run(
                &mut at,
                "create-organization",
                json!({ "display_name": "Author's Organization" }),
            )
            .await;
        let org = id_of(&organization, "organization", "o_");
        let transferred = author
            .run(
                &mut at,
                "transfer",
                json!({ "resource": document, "to": org }),
            )
            .await;
        assert_eq!(id_of(&transferred, "resource", "r_"), document);
        assert_eq!(id_of(&transferred, "to", "o_"), org);
    });
}

#[test]
fn two_registrations_become_one_person_and_come_apart_again() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let first = harness::register(&state, "Twice", "cmd-twice-a@example.invalid").await;
        let second = harness::register(&state, "Twice", "cmd-twice-b@example.invalid").await;
        let mine = Caller::new(&server, &first.token);
        let other = Caller::new(&server, &second.token);
        let (mine_identity, other_identity) = (mine.identity().await, other.identity().await);
        let mut at = 0;

        // A person may propose about a registration they hold, and about
        // nobody else's.
        let proposed = mine
            .run(
                &mut at,
                "propose-match",
                json!({
                    "identity_a": mine_identity,
                    "identity_b": other_identity,
                    "signal": "name_and_group",
                }),
            )
            .await;
        let candidate = id_of(&proposed, "candidate", "m_");

        // Holding a live session on both is the same evidence as signing in
        // twice, so the person proves this one themselves.
        let merged = mine
            .run(
                &mut at,
                "confirm-match",
                json!({ "candidate": candidate, "other_session": second.token }),
            )
            .await;
        assert_eq!(merged["method"], "self");
        assert_eq!(id_of(&merged, "candidate", "m_"), candidate);
        let survivor = id_of(&merged, "survivor", "p_");
        let absorbed = id_of(&merged, "absorbed", "p_");
        assert_ne!(survivor, absorbed);

        // And a merge is not irreversible: the identity detaches onto a person
        // of its own.
        let split = mine
            .run(
                &mut at,
                "split",
                json!({ "identity": other_identity, "evidence": "merged in error" }),
            )
            .await;
        assert_eq!(id_of(&split, "identity", "i_"), other_identity);
        let fresh = id_of(&split, "person", "p_");
        assert_ne!(fresh, survivor);
    });
}

#[test]
fn an_operator_rules_on_a_pair_the_people_cannot_prove() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let operator = harness::register(&state, "Operator", "cmd-operator@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;
        let a = harness::register(&state, "A", "cmd-ruled-a@example.invalid").await;
        let b = harness::register(&state, "B", "cmd-ruled-b@example.invalid").await;
        let operator = Caller::new(&server, &operator.token);
        let (a, b) = (
            Caller::new(&server, &a.token).identity().await,
            Caller::new(&server, &b.token).identity().await,
        );
        let mut at = 0;

        // An operator may propose about strangers; nobody else may.
        let proposed = operator
            .run(
                &mut at,
                "propose-match",
                json!({ "identity_a": a, "identity_b": b, "signal": "name_and_group" }),
            )
            .await;
        let candidate = id_of(&proposed, "candidate", "m_");

        // A ruling that they are two people closes the candidate rather than
        // deleting it, so the same pair does not queue forever.
        let ruled = operator
            .run(
                &mut at,
                "rule-match",
                json!({
                    "candidate": candidate,
                    "same_person": false,
                    "evidence": "two different humans, checked 2026-08-26",
                }),
            )
            .await;
        assert_eq!(id_of(&ruled, "candidate", "m_"), candidate);

        // And the evidence an operator relied on is not something the reply
        // hands back to whoever asks.
        assert!(
            !ruled.to_string().contains("checked"),
            "a ruling echoed its evidence: {ruled}"
        );
    });
}

#[test]
fn the_status_commands_act_on_a_party_and_say_which() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let operator = harness::register(&state, "Warden", "cmd-warden@example.invalid").await;
        harness::operate_platform(&state, operator.person).await;
        let subject = harness::register(&state, "Subject", "cmd-subject@example.invalid").await;
        let operator = Caller::new(&server, &operator.token);
        let party = Caller::new(&server, &subject.token).person().await;
        let mut at = 0;

        let disabled = operator
            .run(
                &mut at,
                "disable",
                json!({ "party": party, "reason": "operator ruling" }),
            )
            .await;
        assert_eq!(id_of(&disabled, "party", "p_"), party);

        let enabled = operator
            .run(&mut at, "enable", json!({ "party": party }))
            .await;
        assert_eq!(id_of(&enabled, "party", "p_"), party);
    });
}
