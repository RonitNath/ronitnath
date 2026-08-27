//! The filters an operator can actually reach, and what each of them costs.
//!
//! `tests/platform.rs` plans the statements the platform queries *name*. This
//! plans the ones they *generate*: every combination of the audit screen's five
//! controls, and the three seeks the one search box can run. Both are things a
//! human composes at runtime, so listing them by hand would list what somebody
//! remembered rather than what the screen can produce.
//!
//! The audit combinations are generated from
//! `platform_filters::CONTROLS` — the same six names the screen sends — so a
//! control added without an index fails the build instead of being noticed in
//! review. `audit` is the change feed, is the audit log, and by ruling has no
//! retention: a filter over it with no index is not slow, it is unusable, and
//! it becomes unusable exactly on the deployment with the most history.

use rn_kernel::store::Value;
use rn_kernel::testing::Local;
use rn_site::api::query::platform_filters::{CONTROLS, Filter};

/// A deployment with one registration in it, which is all a plan needs: the
/// planner reads the schema and the indexes, not the row count.
async fn seeded() -> Local {
    let harness = Local::new();
    harness
        .register("Ronit", "plan-filters@example.test")
        .await
        .expect("registers");
    harness
}

/// Assert that a plan seeks everything it touches.
///
/// A `SCAN` line without `USING` is a table read end to end. `USE TEMP B-TREE`
/// is a sort and is *allowed* here: a filter narrows to a range and then wants
/// it newest-first, and no index can order a range on one column by another.
/// What is forbidden is reading the whole log to answer a question about part
/// of it.
fn seeks(name: &str, plan: &[String]) {
    let joined = plan.join("\n");
    for line in plan {
        assert!(
            !line.contains("SCAN") || line.contains("USING"),
            "{name} scans a table:\n{joined}"
        );
    }
}

/// The combination `mask` names, as the filter the screen would have built.
///
/// One bit per control, in `CONTROLS` order. The values are shaped like a
/// request's — a decoded rowid, a command name from the vocabulary, an instant
/// — because a plan depends on the *shape* of a parameter and not on whether
/// any row matches it.
fn combination(mask: usize) -> (String, Filter) {
    let mut filter = Filter::default();
    let mut named: Vec<&str> = Vec::new();
    for (bit, control) in CONTROLS.iter().enumerate() {
        if mask & (1 << bit) == 0 {
            continue;
        }
        named.push(control);
        match *control {
            "actor" => filter.actor = Some(1),
            "hat" => filter.hat = Some(2),
            "object" => filter.object = Some((rn_kernel::audit::object::PARTY, 3)),
            "command" => filter.command = Some("share".to_owned()),
            "from" => filter.from = Some(1_800_000_000),
            "to" => filter.to = Some(1_800_086_400),
            other => panic!("{other} is a control with no value in this test"),
        }
    }
    let name = if named.is_empty() {
        "no filter".to_owned()
    } else {
        named.join("+")
    };
    (name, filter)
}

async fn plan(harness: &Local, sql: &str, params: Vec<Value>) -> Vec<String> {
    let plan = harness
        .store()
        .engine()
        .explain(sql, params)
        .unwrap_or_else(|error| panic!("EXPLAIN QUERY PLAN: {error}"));
    assert!(!plan.is_empty(), "a statement produced no plan");
    plan
}

/// Every combination of the audit screen's controls, generated from the
/// screen's own list.
#[tokio::test]
async fn explain_every_audit_filter_combination_is_an_index_seek() {
    let harness = seeded().await;
    let total = 1usize << CONTROLS.len();
    assert_eq!(total, 64, "six controls, sixty-four combinations");
    for mask in 0..total {
        let (name, filter) = combination(mask);
        let (sql, args) = filter.plan(i64::MAX, 100);
        seeks(&name, &plan(&harness, sql, args).await);
    }
}

/// And each of the six leads is reached by the combination that should reach
/// it — a plan that seeks is worth nothing if every combination fell through
/// to the same unfiltered statement.
#[tokio::test]
async fn each_lead_is_reached_and_rides_the_index_it_was_added_for() {
    use rn_site::api::query::platform_filters as filters;
    let harness = seeded().await;
    let rides = async |filter: Filter, index: &str, what: &str| {
        let (sql, args) = filter.plan(i64::MAX, 100);
        let joined = plan(&harness, sql, args).await.join("\n");
        assert!(
            joined.contains(index),
            "{what} does not ride {index}:\n{joined}"
        );
    };

    // The actor index is migration 1's; the hat, the time range and the object
    // side table are migration 7's; the command index is migration 9's.
    rides(
        Filter {
            actor: Some(1),
            ..Filter::default()
        },
        "audit_actor_idx",
        "the actor filter",
    )
    .await;
    rides(
        Filter {
            hat: Some(1),
            ..Filter::default()
        },
        "audit_acting_as_idx",
        "the hat filter",
    )
    .await;
    rides(
        Filter {
            command: Some("share".to_owned()),
            ..Filter::default()
        },
        "audit_command_idx",
        "the command filter",
    )
    .await;
    rides(
        Filter {
            from: Some(1_800_000_000),
            ..Filter::default()
        },
        "audit_at_idx",
        "the time range",
    )
    .await;
    rides(
        Filter {
            object: Some((rn_kernel::audit::object::PARTY, 1)),
            ..Filter::default()
        },
        // The side table is WITHOUT ROWID, so its primary key *is* its
        // storage: `(kind, id, audit_id)` seeks the object and bounds the
        // walk by the offset in one read.
        "SEARCH o USING PRIMARY KEY",
        "the object filter",
    )
    .await;
    // And with nothing set, the primary key walked backwards — the live tail.
    let joined = plan(
        &harness,
        filters::TAIL,
        Filter::default().plan(i64::MAX, 100).1,
    )
    .await
    .join("\n");
    assert!(
        joined.contains("SEARCH a USING INTEGER PRIMARY KEY"),
        "the unfiltered tail must be a rowid range:\n{joined}"
    );
}

/// The one search box: three statements, and the plan for each.
///
/// Printed as well as asserted, because "it is a seek" is a claim and the plan
/// is the evidence — the leg's report carries this output verbatim.
#[tokio::test]
async fn explain_the_three_find_statements_are_each_one_seek() {
    use rn_site::api::query::platform::statements;
    let harness = seeded().await;
    let listed = statements();
    let mut seen = 0;
    for (name, sql, args) in listed {
        if !name.starts_with("find.") {
            continue;
        }
        seen += 1;
        let plan = plan(&harness, sql, args).await;
        println!("{name}:\n{}", plan.join("\n"));
        seeks(name, &plan);
    }
    assert_eq!(
        seen, 4,
        "the box runs three seeks, and an identity id resolves to its person"
    );
}
