//! The merge budget: ≤ 50 ms at 10⁴ affected rows.
//!
//! The kernel report's number is p99 on a real three-node cluster; what a
//! laptop measures honestly is the single-node floor — the part of the number
//! that is the *statements'* rather than the network's. A regression here is a
//! regression there; a pass here is not a pass there, and `tools/perf` on a
//! real cluster is what closes that gap.
//!
//! Ten thousand affected rows is 5 000 memberships plus 5 000 grants held by
//! the absorbed person: somebody who has been in this system for years. What
//! is timed is one command — `RuleMatch` reads the candidate, plans, and
//! commits the five steps as one batch — and nothing else. A merge can only
//! happen once to a pair, so every iteration builds its own database, outside
//! the timed region.

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use rn_api::commands::{MatchSignal, RuleMatch};
use rn_kernel::bind;
use rn_kernel::ids::{Id, MatchCandidate};
use rn_kernel::merge::{candidate, rule};
use rn_kernel::principal::Principal;
use rn_kernel::store::{Count, Reads, Value};
use rn_kernel::testing::{Local, TEST_EPOCH};

/// The report's budget for a merge at 10⁴ affected rows.
const BUDGET: Duration = Duration::from_millis(50);

/// Memberships on the absorbed person, and grants on it: 10⁴ rows in all.
const ROWS: i64 = 5_000;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime starts")
}

/// A database with a merge waiting to happen.
struct Prepared {
    harness: Local,
    operator: Principal,
    candidate: Id<MatchCandidate>,
}

async fn one(harness: &Local, sql: &'static str, params: Vec<Value>) {
    harness
        .store()
        .execute(sql, params)
        .await
        .expect("the statement runs");
}

async fn number(harness: &Local, sql: &'static str, params: Vec<Value>) -> i64 {
    harness
        .store()
        .query::<Count>(sql, params)
        .await
        .expect("query runs")
        .first()
        .expect("a row")
        .0
}

/// An operator, two people, 5 000 memberships and 5 000 grants on the one to
/// be absorbed, and a candidate for the pair.
async fn prepare() -> Prepared {
    let harness = Local::new();
    let operator = harness
        .register("Operator", "bench-op@example.test")
        .await
        .expect("registers");
    rule::make_operator(
        harness.store(),
        operator.principal.acting_as().expect("a member"),
    )
    .await
    .expect("the operator row is written");

    let first = harness
        .register("Ronit", "bench-a@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit", "bench-b@example.test")
        .await
        .expect("registers");
    let absorbed = second.principal.acting_as().expect("a member");

    // Generated in three statements rather than ten thousand round trips:
    // what is being measured is the merge, not the fixture.
    one(
        &harness,
        "INSERT INTO party (kind, display_name, status, created_at) \
         WITH RECURSIVE n(v) AS (SELECT 1 UNION ALL SELECT v + 1 FROM n WHERE v < $1) \
         SELECT 'group', 'g' || v, 'active', 0 FROM n",
        bind![ROWS],
    )
    .await;
    one(
        &harness,
        "INSERT INTO membership (group_id, party_id, role, at) \
         SELECT id, $1, 'member', 0 FROM party WHERE kind = 'group'",
        bind![absorbed],
    )
    .await;
    one(
        &harness,
        "INSERT INTO relation \
         (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
         WITH RECURSIVE n(v) AS (SELECT 1 UNION ALL SELECT v + 1 FROM n WHERE v < $1) \
         SELECT 'document', v, 'editor', 'person', $2, NULL, 0 FROM n",
        bind![ROWS, absorbed],
    )
    .await;

    let (a, b) = (
        first.principal.identity().expect("a member"),
        second.principal.identity().expect("a member"),
    );
    one(
        &harness,
        candidate::PROPOSE_QUIETLY,
        candidate::proposal(a, b, MatchSignal::OidcSubject, TEST_EPOCH),
    )
    .await;
    let queued = number(
        &harness,
        "SELECT id AS n FROM match_candidate WHERE identity_a = $1 AND identity_b = $2",
        bind![a.min(b), a.max(b)],
    )
    .await;

    // The fixture is part of what the budget means: a bench that has stopped
    // moving 10⁴ rows would only get faster and say nothing.
    let affected = number(&harness, "SELECT count(*) AS n FROM membership", bind![]).await
        + number(
            &harness,
            "SELECT count(*) AS n FROM relation WHERE subject_kind = 'person' \
             AND subject_id = $1",
            bind![absorbed],
        )
        .await;
    assert!(
        affected >= ROWS * 2,
        "the bench must move 10⁴ rows, not {affected}"
    );

    Prepared {
        harness,
        operator: operator.principal,
        candidate: Id::new(queued),
    }
}

/// One merge, timed and nothing else.
async fn merge_once(prepared: Prepared) -> Duration {
    let args = RuleMatch {
        candidate: prepared.candidate.public(prepared.harness.store().ids()),
        same_person: true,
        evidence: "a bench, not a person".into(),
    };
    let ctx = prepared.harness.ctx(prepared.operator.clone());
    let started = Instant::now();
    rn_kernel::cmd::rule_match(&ctx, &args)
        .await
        .expect("the merge runs");
    started.elapsed()
}

fn merge(c: &mut Criterion) {
    let rt = runtime();

    // The line a person running the suite actually reads. Criterion's own
    // report is in `target/criterion`.
    let each = rt.block_on(async { merge_once(prepare().await).await });
    let verdict = if each <= BUDGET { "within" } else { "OVER" };
    println!(
        "  budget  merge at {} affected rows: {each:?}, {verdict} {BUDGET:?}",
        ROWS * 2
    );

    let mut group = c.benchmark_group("merge");
    group.sample_size(10);
    group.bench_function("ten thousand affected rows", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let prepared = rt.block_on(prepare());
                total += rt.block_on(merge_once(prepared));
            }
            total
        });
    });
    group.finish();
}

criterion_group!(benches, merge);
criterion_main!(benches);
