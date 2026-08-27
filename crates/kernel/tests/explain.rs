//! The "no full scan" gate: every hot query under `EXPLAIN QUERY PLAN`,
//! asserting the index it must ride.
//!
//! These are the four queries every authenticated request and every command
//! runs. A missing index does not fail a functional test — it makes one
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
