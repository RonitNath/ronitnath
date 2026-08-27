//! `check()` against the kernel report's budget: ≤ 300 µs cold, ≤ 20 µs warm,
//! at 10⁶ relations, in the shapes an adversary can actually produce.
//!
//! Two shapes matter and they are different threats.
//!
//! **An object with 10⁴ grants** is the one a stranger chooses: sharing
//! something with everybody costs them nothing and would cost every later
//! `check` a pass over the object's rows — if the query were keyed on the
//! object. It is keyed on the subject, so it is not.
//!
//! **A subject in 10⁴ groups** is the one the deployment chooses. It cannot be
//! made free: a principal that stands for ten thousand subjects is ten
//! thousand things to look up, and the expansion that produced the list has
//! already paid the same order. It is measured and printed rather than hidden,
//! and the number to read next to it is the one for an ordinary principal.
//!
//! Cold and warm here are the report's meanings: cold includes expanding the
//! principal (`principal::expand`, one query, cached per session digest); warm
//! is the check itself with the subject set already in hand.

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey};
use rn_kernel::principal::SubjectSet;
use rn_kernel::relation::{self, Object, Relation};
use rn_kernel::store::{Clock, Sqlite, Store};
use rn_kernel::testing::{Harness, Local, TEST_EPOCH, TEST_ID_KEY};

/// ≤ 300 µs cold, from the kernel report's performance gates.
const COLD_BUDGET: Duration = Duration::from_micros(300);
/// ≤ 20 µs warm.
const WARM_BUDGET: Duration = Duration::from_micros(20);

/// How many relation rows the store holds.
const RELATIONS: i64 = 1_000_000;
/// How many grants sit on the one heavily shared object.
const FAN_OUT: i64 = 10_000;
/// How many groups the pathological principal belongs to.
const WIDE: i64 = 10_000;
/// How many groups an ordinary principal belongs to.
const ORDINARY: i64 = 8;
/// The first ordinary group's party id. Kept clear of the two people.
const FIRST_GROUP: i64 = 11;

/// The object every measurement asks about: the one with 10⁴ grants on it.
const HOT: i64 = 42;
/// The person doing the asking.
const ASKER: i64 = 1;
/// The person who is in ten thousand groups.
const WIDE_ASKER: i64 = 2;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime starts")
}

fn report(name: &str, elapsed: Duration, iterations: u32, budget: Duration) {
    let each = elapsed / iterations;
    let verdict = if each <= budget { "within" } else { "OVER" };
    println!("  budget  {name}: {each:?} per op, {verdict} {budget:?}");
}

/// The parties every fixture below points at. `membership` has foreign keys,
/// so the groups have to exist as rows rather than as integers.
const PEOPLE: &str = "INSERT INTO party (id, kind, display_name, status, created_at) \
     VALUES (1, 'person', 'Asker', 'active', 1), (2, 'person', 'Wide', 'active', 1)";

const GROUPS: &str = "WITH RECURSIVE seq(n) AS (SELECT 0 UNION ALL SELECT n + 1 FROM seq \
     WHERE n < $1) \
     INSERT INTO party (id, kind, display_name, status, created_at) \
     SELECT $2 + n, 'group', 'A group', 'active', 1 FROM seq";

const WIDE_GROUPS: &str = "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq \
     WHERE n < $1) \
     INSERT INTO party (id, kind, display_name, status, created_at) \
     SELECT 500000 + n, 'group', 'A group', 'active', 1 FROM seq";

/// The background: 10⁶ grants spread over documents nobody in this benchmark
/// is asking about, so the index has to be a real index rather than a table
/// small enough to scan.
const BACKGROUND: &str = "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq \
     WHERE n < $1) \
     INSERT INTO relation (object_kind, object_id, relation, subject_kind, subject_id, at) \
     SELECT 'document', 1000 + (n % 200000), 'viewer', 'group', 100000 + n, 1 FROM seq";

/// 10⁴ grants on one object, to a group nobody asking is in.
const FAN: &str = "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq \
     WHERE n < $1) \
     INSERT INTO relation (object_kind, object_id, relation, subject_kind, subject_id, at) \
     SELECT 'document', $2, 'viewer', 'group', 900000 + n, 1 FROM seq";

/// One person in a great many groups.
const WIDE_MEMBERSHIP: &str = "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq \
     WHERE n < $1) \
     INSERT INTO membership (group_id, party_id, role, at) \
     SELECT 500000 + n, $2, 'member', 1 FROM seq";

/// The seeded world, on a real file so a cold read reaches a disk.
struct World {
    harness: Local,
    _dir: tempfile::TempDir,
}

async fn world() -> World {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let store = Store::new(
        Sqlite::open(&dir.path().join("bench.sqlite")).expect("the database opens"),
        IdKey::from_hex(TEST_ID_KEY).expect("the test key is well formed"),
        Clock::fixed(TEST_EPOCH),
    );
    let harness = Harness::over(store);

    for (sql, params) in [
        (PEOPLE, bind![]),
        (GROUPS, bind![ORDINARY, FIRST_GROUP]),
        (WIDE_GROUPS, bind![WIDE]),
    ] {
        harness
            .store()
            .execute(sql, params)
            .await
            .expect("the parties land");
    }
    harness
        .store()
        .execute(BACKGROUND, bind![RELATIONS - FAN_OUT])
        .await
        .expect("the background lands");
    harness
        .store()
        .execute(FAN, bind![FAN_OUT, HOT])
        .await
        .expect("the fan-out lands");
    harness
        .store()
        .execute(WIDE_MEMBERSHIP, bind![WIDE, WIDE_ASKER])
        .await
        .expect("the wide membership lands");

    // The one grant the ordinary principal actually reaches the object by.
    harness
        .store()
        .execute(
            "INSERT INTO relation (object_kind, object_id, relation, subject_kind, subject_id, at) \
             VALUES ('document', $1, 'editor', 'group', $2, 1)",
            bind![HOT, FIRST_GROUP],
        )
        .await
        .expect("the grant lands");

    World { harness, _dir: dir }
}

/// A principal standing for `count` groups starting at `first`, the way
/// `expand` would produce it.
fn subjects(person: i64, first: i64, count: i64) -> SubjectSet {
    SubjectSet {
        identity: Some(Id::new(person)),
        person: Some(Id::new(person)),
        organizations: Vec::new(),
        groups: (0..count).map(|n| Id::new(first + n)).collect(),
        link: None,
        authenticated: true,
    }
}

/// The principal a session resolves to, for the paths that expand it here.
fn member(person: i64) -> rn_kernel::principal::Principal {
    rn_kernel::principal::Principal::Member {
        identity: Id::new(person),
        person: Some(Id::new(person)),
        acting_as: Id::new(person),
        session: Id::new(1),
        impersonated_by: None,
    }
}

fn check(c: &mut Criterion) {
    let rt = runtime();
    let world = rt.block_on(world());
    let store = world.harness.store();
    let object = Object::document(Id::new(HOT));
    let ordinary = subjects(ASKER, FIRST_GROUP, ORDINARY);
    // The pathological principal reaches the object through none of its ten
    // thousand groups: the worst case is the one that has to look at all of
    // them before answering.
    let wide = subjects(WIDE_ASKER, 500_001, WIDE);

    assert!(
        rt.block_on(relation::check(store, &ordinary, Relation::Viewer, object))
            .expect("checks"),
        "the ordinary principal reaches the object"
    );

    c.bench_function("check/ordinary", |b| {
        b.to_async(&rt).iter(|| async {
            relation::check(store, &ordinary, Relation::Viewer, object)
                .await
                .expect("checks")
        });
    });
    c.bench_function("check/wide", |b| {
        b.to_async(&rt).iter(|| async {
            relation::check(store, &wide, Relation::Viewer, object)
                .await
                .expect("checks")
        });
    });

    // Cold: the first ask of a request, expansion included.
    let started = Instant::now();
    rt.block_on(async {
        for _ in 0..500 {
            let set = rn_kernel::principal::expand(store, &member(ASKER))
                .await
                .expect("expands");
            std::hint::black_box(
                relation::check(store, &set, Relation::Viewer, object)
                    .await
                    .expect("checks"),
            );
        }
    });
    report(
        "check cold (expand + check)",
        started.elapsed(),
        500,
        COLD_BUDGET,
    );

    let started = Instant::now();
    rt.block_on(async {
        for _ in 0..20_000 {
            std::hint::black_box(
                relation::check(store, &ordinary, Relation::Viewer, object)
                    .await
                    .expect("checks"),
            );
        }
    });
    report("check warm", started.elapsed(), 20_000, WARM_BUDGET);

    let started = Instant::now();
    rt.block_on(async {
        for _ in 0..200 {
            std::hint::black_box(
                relation::check(store, &wide, Relation::Viewer, object)
                    .await
                    .expect("checks"),
            );
        }
    });
    // Printed against the same budget, and expected to exceed it: ten thousand
    // subjects is ten thousand seeks. The number beneath it is the evidence
    // that this is a property of the principal rather than of the query —
    // building the subject set costs the same order before `check` is called
    // at all, and no index makes ten thousand lookups into one.
    report(
        "check warm, subject in 10⁴ groups",
        started.elapsed(),
        200,
        WARM_BUDGET,
    );

    let started = Instant::now();
    rt.block_on(async {
        for _ in 0..200 {
            std::hint::black_box(
                rn_kernel::principal::expand(store, &member(WIDE_ASKER))
                    .await
                    .expect("expands"),
            );
        }
    });
    report(
        "expand alone, subject in 10⁴ groups",
        started.elapsed(),
        200,
        COLD_BUDGET,
    );
}

criterion_group!(benches, check);
criterion_main!(benches);
