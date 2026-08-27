//! The "no full scan" gate: every hot query under `EXPLAIN QUERY PLAN`,
//! asserting the index it must ride.
//!
//! These are the queries every authenticated request, every command and every
//! hot page runs. A missing index does not fail a functional test — it makes one
//! slightly slower on ten rows and unusable on ten million — so the plan is
//! what gets asserted, not the result.

use rn_kernel::store::Value;
use rn_kernel::testing::Local;
use rn_kernel::{audit, bind, cmd, feed, principal};

/// Assert that a plan reaches the named index and scans nothing.
fn rides(plan: &[String], index: &str) {
    let joined = plan.join("\n");
    assert!(
        joined.contains(index),
        "the plan does not use {index}:\n{joined}"
    );
    for line in plan {
        assert!(
            !line.contains("SCAN") || line.contains("USING"),
            "a full scan crept into a hot query:\n{joined}"
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

#[tokio::test]
async fn explain_session_resolve_rides_the_token_hash_index() {
    let harness = Local::new();
    harness
        .register("Ronit", "plan@example.test")
        .await
        .expect("registers");
    let plan = plan(&harness, principal::RESOLVE_SQL, bind![vec![0u8; 32]]).await;
    // The UNIQUE constraint on `session.token_hash` is what makes the whole
    // authenticated read path a seek rather than a table scan.
    rides(&plan, "session");
    assert!(
        plan.join("\n").contains("sqlite_autoindex_session_1")
            || plan.join("\n").contains("token_hash"),
        "resolve must reach the session by its token digest:\n{}",
        plan.join("\n")
    );
}

#[tokio::test]
async fn explain_sign_in_rides_the_partial_email_index() {
    let harness = Local::new();
    harness
        .register("Ronit", "plan-in@example.test")
        .await
        .expect("registers");
    let plan = plan(&harness, cmd::SIGN_IN_SQL, bind!["plan-in@example.test"]).await;
    rides(&plan, "factor_email_unique_idx");
}

#[tokio::test]
async fn explain_audit_by_key_rides_the_unique_key_index() {
    let harness = Local::new();
    harness
        .register("Ronit", "plan-key@example.test")
        .await
        .expect("registers");
    // Every command runs this twice — once before planning and once after
    // committing — so a scan here would be two scans per write.
    let plan = plan(
        &harness,
        audit::BY_KEY_SQL,
        bind!["00000000-0000-0000-0000-000000000000"],
    )
    .await;
    rides(&plan, "audit");
    assert!(
        plan.join("\n").contains("sqlite_autoindex_audit_1") || plan.join("\n").contains("key"),
        "the idempotency lookup must reach the row by key:\n{}",
        plan.join("\n")
    );
}

#[tokio::test]
async fn explain_feed_read_is_a_range_over_the_offset() {
    let harness = Local::new();
    harness
        .register("Ronit", "plan-feed@example.test")
        .await
        .expect("registers");
    let plan = plan(&harness, feed::READ_SQL, bind![0i64, 16i64]).await;
    let joined = plan.join("\n");
    // `id > ?` over the rowid primary key: a range seek, and no sort, because
    // rowid order is already the order the feed wants.
    assert!(
        joined.contains("SEARCH audit USING INTEGER PRIMARY KEY"),
        "the feed read must be a rowid range:\n{joined}"
    );
    assert!(
        !joined.contains("TEMP B-TREE"),
        "the feed must not sort what the primary key already orders:\n{joined}"
    );
}

#[tokio::test]
async fn explain_membership_expansion_rides_the_party_index() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "plan-expand@example.test")
        .await
        .expect("registers");
    let plan = plan(
        &harness,
        "SELECT m.group_id, p.kind FROM membership m \
         JOIN party p ON p.id = m.group_id WHERE m.party_id = $1",
        bind![who.principal.acting_as().expect("acts as somebody")],
    )
    .await;
    rides(&plan, "membership_party_idx");
}

// --------------------------------------------------------------- merge ---
//
// The two reads a merged deployment does on every request that names a person
// or opens the queue: following an alias, and asking what is proposed about an
// identity. Both are seeks, and both are asserted here rather than assumed,
// because a merge-heavy deployment runs them constantly and neither would fail
// a functional test if the index went away.

#[tokio::test]
async fn explain_alias_resolution_rides_the_primary_key() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "plan-alias@example.test")
        .await
        .expect("registers");
    let plan = plan(
        &harness,
        rn_kernel::merge::ALIAS_SQL,
        bind![who.principal.acting_as().expect("acts as somebody")],
    )
    .await;
    let joined = plan.join("\n");
    // `old_person_id` is an INTEGER PRIMARY KEY, so it *is* the rowid: one hop
    // of the chain is the cheapest lookup SQLite has.
    assert!(
        joined.contains("SEARCH person_alias USING INTEGER PRIMARY KEY"),
        "an alias hop must be a rowid seek:\n{joined}"
    );
}

#[tokio::test]
async fn explain_the_match_queue_rides_the_identity_index() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "plan-queue@example.test")
        .await
        .expect("registers");
    let identity = who.principal.identity().expect("a member");
    let by_identity = plan(
        &harness,
        rn_kernel::merge::candidate::BY_IDENTITY_SQL,
        bind![identity],
    )
    .await;
    rides(&by_identity, "match_candidate_a_idx");

    // And one candidate by its row, which every proof reads first.
    let by_id = plan(
        &harness,
        rn_kernel::merge::candidate::BY_ID_SQL,
        bind![1i64],
    )
    .await;
    assert!(
        by_id.join("\n").contains("INTEGER PRIMARY KEY"),
        "a candidate is read by its rowid:\n{}",
        by_id.join("\n")
    );
}

#[tokio::test]
async fn explain_the_signal_scan_rides_the_verified_value_index() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "plan-scan@example.test")
        .await
        .expect("registers");
    let plan = plan(
        &harness,
        rn_kernel::merge::scan::SHARED_FACTOR_SQL,
        bind![who.principal.identity().expect("a member")],
    )
    .await;
    // Without this index, every registration asks "who else has proven this
    // value" by reading every factor in the deployment.
    rides(&plan, "factor_verified_value_idx");
}

#[tokio::test]
async fn explain_minted_invitations_rides_the_granted_by_index() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "plan-minted@example.test")
        .await
        .expect("registers");
    let identity = who.principal.identity().expect("a member has one");
    // The member tier's "invitations I minted": the filter is `granted_by`,
    // and without an index on it this is a scan of every relation row in the
    // deployment — a table that grows with every share and every membership.
    let plan = plan(
        &harness,
        "SELECT l.id FROM relation rel JOIN link l ON l.id = rel.subject_id \
         WHERE rel.subject_kind = 'link' AND rel.granted_by = $1",
        bind![identity],
    )
    .await;
    rides(&plan, "relation_granted_by_idx");
}

#[tokio::test]
async fn explain_describing_an_invitation_seeks_the_link_and_its_grant() {
    let harness = Local::new();
    harness
        .register("Ronit", "plan-describe@example.test")
        .await
        .expect("registers");
    // The claim page runs this for every reader who opens a link, signed in or
    // not, which makes it as public as sign-in is. The link is a rowid seek and
    // the grant rides the subject index; the two display-name joins are primary
    // keys.
    let plan = plan(&harness, rn_kernel::invite::DESCRIBE_SQL, bind![1_i64]).await;
    rides(&plan, "relation_subject");
}

// ---------------------------------------------------- the OpenID Provider ---
//
// Four lookups run on routes anybody may post a guess to, and one runs on
// every claim the Provider makes. All four are seeks, and these are what keep
// them so: a bearer token presented at `userinfo` costs one index probe
// whether or not it is real, which is also what stops the endpoint being an
// oracle that answers faster for a token that exists.

#[tokio::test]
async fn explain_an_access_token_is_found_by_its_digest_alone() {
    let harness = Local::new();
    let plan = plan(
        &harness,
        rn_kernel::oidc::token::BY_HASH_SQL,
        bind![vec![0u8; 32]],
    )
    .await;
    rides(&plan, "oidc_token");
    assert!(
        plan.join("\n").contains("sqlite_autoindex_oidc_token_1")
            || plan.join("\n").contains("token_hash"),
        "a token must be reached by its digest:\n{}",
        plan.join("\n")
    );
}

#[tokio::test]
async fn explain_an_authorization_code_is_found_by_its_digest_alone() {
    let harness = Local::new();
    let plan = plan(
        &harness,
        rn_kernel::oidc::code::BY_HASH_SQL,
        bind![vec![0u8; 32]],
    )
    .await;
    rides(&plan, "oidc_code");
    assert!(
        plan.join("\n").contains("sqlite_autoindex_oidc_code_1")
            || plan.join("\n").contains("code_hash"),
        "a code must be reached by its digest:\n{}",
        plan.join("\n")
    );
}

#[tokio::test]
async fn explain_a_subject_is_one_seek_on_the_sector_and_the_person() {
    let harness = Local::new();
    let pair = plan(
        &harness,
        rn_kernel::oidc::subject::BY_PAIR_SQL,
        bind![0_i64, 1_i64],
    )
    .await;
    rides(&pair, "oidc_subject_pair_idx");

    // And the reverse — which person a `sub` names — rides the other unique
    // index, with the merge join hanging off `person_alias`'s primary key.
    let reverse = plan(&harness, rn_kernel::oidc::subject::BY_SUB_SQL, bind!["s"]).await;
    rides(&reverse, "oidc_subject_sub_idx");
}

#[tokio::test]
async fn explain_a_consent_is_the_primary_key_of_the_pair_it_is_about() {
    let harness = Local::new();
    let plan = plan(
        &harness,
        rn_kernel::oidc::consent::BY_PAIR_SQL,
        bind![1_i64, 1_i64],
    )
    .await;
    rides(&plan, "oidc_consent");
}

#[tokio::test]
async fn explain_a_handle_lookup_rides_the_unique_index() {
    let harness = Local::new();
    let plan = plan(&harness, rn_kernel::oidc::handle::TAKEN_SQL, bind!["ronit"]).await;
    let joined = plan.join("\n");
    assert!(
        joined.contains("party_handle_unique_idx"),
        "a live handle must be a seek:\n{joined}"
    );
    assert!(
        joined.contains("party_handle_alias"),
        "and so must one a merge filed away:\n{joined}"
    );
}

#[tokio::test]
async fn explain_the_clients_a_session_has_to_be_told_about_is_a_seek() {
    let harness = Local::new();
    let plan = plan(
        &harness,
        rn_kernel::oidc::token::LOGOUT_TARGETS_SQL,
        bind![1_i64],
    )
    .await;
    rides(&plan, "oidc_token_session_idx");
}
