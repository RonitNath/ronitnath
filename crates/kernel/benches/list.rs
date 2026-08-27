//! `list_visible()` against the kernel report's budget: ≤ 5 ms for 10⁴
//! objects.
//!
//! The shape being measured is "everything I can see", which is two questions
//! joined: what I own, and what has been shared with me. Both sides are index
//! seeks and the page comes off `resource_kind_created_idx` in the order it is
//! already in — but the candidate set is still every object the principal can
//! reach, so 10⁴ objects is 10⁴ ids being merged before a page of fifty comes
//! back. That is the number the budget is about.
//!
//! Two principals are measured: the one who owns all ten thousand, and the one
//! who reaches a few through a group. The second is the common case and the
//! first is the one that has to stay inside the budget.

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey};
use rn_kernel::principal::SubjectSet;
use rn_kernel::relation::{self, Page};
use rn_kernel::resource::kinds;
use rn_kernel::store::{Clock, Sqlite, Store};
use rn_kernel::testing::{Harness, Local, TEST_EPOCH, TEST_ID_KEY};

/// ≤ 5 ms for 10⁴ objects.
const BUDGET: Duration = Duration::from_millis(5);

/// How many documents the owner owns.
const OBJECTS: i64 = 10_000;
/// How many of them are shared with the group.
const SHARED: i64 = 100;
/// How many relation rows sit around them, on other people's things.
const BACKGROUND_ROWS: i64 = 200_000;

const OWNER: i64 = 1;
const READER: i64 = 2;
const TEAM: i64 = 3;

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

const PARTIES: &str = "INSERT INTO party (id, kind, display_name, status, created_at) \
     VALUES (1, 'person', 'Owner', 'active', 1), (2, 'person', 'Reader', 'active', 1), \
            (3, 'group', 'Team', 'active', 1)";

const DOCUMENTS: &str = "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq \
     WHERE n < $1) \
     INSERT INTO resource (id, kind, owner_party_id, home_zone, status, created_at) \
     SELECT n, 'document', 1, 'us-west', 'published', 1800000000 + n FROM seq";

const OWNERSHIP: &str = "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq \
     WHERE n < $1) \
     INSERT INTO relation (object_kind, object_id, relation, subject_kind, subject_id, at) \
     SELECT 'document', n, 'owner', 'person', 1, 1 FROM seq";

const SHARES: &str = "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq \
     WHERE n < $1) \
     INSERT INTO relation (object_kind, object_id, relation, subject_kind, subject_id, at) \
     SELECT 'document', n * 7, 'viewer', 'group', 3, 1 FROM seq";

const BACKGROUND: &str = "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq \
     WHERE n < $1) \
     INSERT INTO relation (object_kind, object_id, relation, subject_kind, subject_id, at) \
     SELECT 'document', 1000000 + n, 'viewer', 'group', 100000 + n, 1 FROM seq";

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
        (PARTIES, bind![]),
        (DOCUMENTS, bind![OBJECTS]),
        (OWNERSHIP, bind![OBJECTS]),
        (SHARES, bind![SHARED]),
        (BACKGROUND, bind![BACKGROUND_ROWS]),
    ] {
        harness
            .store()
            .execute(sql, params)
            .await
            .expect("the fixture lands");
    }
    World { harness, _dir: dir }
}

fn subjects(person: i64, groups: &[i64]) -> SubjectSet {
    SubjectSet {
        identity: Some(Id::new(person)),
        person: Some(Id::new(person)),
        organizations: Vec::new(),
        groups: groups.iter().map(|g| Id::new(*g)).collect(),
        link: None,
        authenticated: true,
    }
}

fn list(c: &mut Criterion) {
    let rt = runtime();
    let world = rt.block_on(world());
    let store = world.harness.store();
    let owner = subjects(OWNER, &[]);
    let reader = subjects(READER, &[TEAM]);

    let first = rt
        .block_on(relation::list_visible(
            store,
            &owner,
            kinds::DOCUMENT,
            Page::default(),
        ))
        .expect("lists");
    assert_eq!(first.len(), Page::DEFAULT_LIMIT);
    let deep = Page::after(&first[first.len() - 1], Page::DEFAULT_LIMIT);

    c.bench_function("list_visible/owner/first-page", |b| {
        b.to_async(&rt).iter(|| async {
            relation::list_visible(store, &owner, kinds::DOCUMENT, Page::default())
                .await
                .expect("lists")
        });
    });
    c.bench_function("list_visible/owner/second-page", |b| {
        b.to_async(&rt).iter(|| async {
            relation::list_visible(store, &owner, kinds::DOCUMENT, deep)
                .await
                .expect("lists")
        });
    });
    c.bench_function("list_visible/reader/first-page", |b| {
        b.to_async(&rt).iter(|| async {
            relation::list_visible(store, &reader, kinds::DOCUMENT, Page::default())
                .await
                .expect("lists")
        });
    });

    for (name, set, page) in [
        ("list_visible owner, first page", &owner, Page::default()),
        ("list_visible owner, second page", &owner, deep),
        ("list_visible reader, first page", &reader, Page::default()),
    ] {
        let started = Instant::now();
        rt.block_on(async {
            for _ in 0..200 {
                std::hint::black_box(
                    relation::list_visible(store, set, kinds::DOCUMENT, page)
                        .await
                        .expect("lists"),
                );
            }
        });
        report(name, started.elapsed(), 200, BUDGET);
    }
}

criterion_group!(benches, list);
criterion_main!(benches);
