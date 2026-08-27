//! The three statements that move ownership, under `EXPLAIN QUERY PLAN`.
//!
//! A merge and a split each touch the two tables where "who owns this" lives,
//! and both are addressed by a party rather than by a row id — the shape that
//! is a full table scan if it misses its index. Ten resources make that
//! invisible and ten million make it an outage, so the plan is what is
//! asserted here and the behaviour is asserted elsewhere.

use super::prelude::*;

/// Every step of the plan is a seek, and it reaches the named index.
fn seeks(plan: &[String], index: &str) {
    let joined = plan.join("\n");
    assert!(
        joined.contains(index),
        "the plan does not use {index}:\n{joined}"
    );
    for line in plan {
        assert!(
            !line.contains("SCAN") || line.contains("USING"),
            "a full scan crept into an ownership statement:\n{joined}"
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

#[tokio::test]
async fn ownership_moves_by_index_seek_in_both_directions() {
    let harness = Local::new();

    // Merge: everything the absorbed person owns, found through the owner
    // index rather than by reading the registry.
    seeks(
        &plan(&harness, crate::merge::apply::RESOURCES, bind![1, 2]),
        "resource_owner_idx",
    );

    // Split: the same seek on the way back, plus the `#owner` rows of the
    // person the identity came from — which is why the subject is asked for as
    // a `subject_key` and not as its two columns.
    let restore = plan(&harness, crate::merge::split::RESOURCES, bind![1, 2, 3]);
    seeks(&restore, "resource_owner_idx");
    seeks(&restore, "relation_subject_key_idx");

    // And the survivor's stale `#owner` row, removed by the same index.
    let drop_owner = plan(&harness, crate::merge::split::DROP_OWNER, bind![1, 2]);
    seeks(&drop_owner, "relation_subject_key_idx");
    seeks(&drop_owner, "resource_owner_idx");
}
