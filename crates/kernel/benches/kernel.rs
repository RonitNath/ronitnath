//! The kernel report's budgets, measured.
//!
//! | path | budget |
//! |---|---|
//! | principal resolution | ≤ 2 ms cold · 50 µs warm |
//! | id encode / decode | not budgeted; measured because everything else pays it |
//! | command commit | ≤ 1 intra-zone RTT + 1 ms |
//!
//! The budgets in the report are p99 on a real three-node cluster; what a
//! laptop can honestly measure is the single-node floor, which is the part of
//! the number that is *ours* rather than the network's. A regression here is a
//! regression there; a pass here is not a pass there, and `tools/perf` on a
//! real cluster is what closes that gap.
//!
//! Every benchmark prints its median against its budget on the way out, so the
//! number is in the run's output and not only in criterion's report directory.

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use rn_api::commands::Register;
use rn_kernel::cmd;
use rn_kernel::ids::{Id, IdKey, Identity};
use rn_kernel::principal::{Principal, PrincipalCache, resolve, resolve_cached};
use rn_kernel::testing::{Local, Node, TEST_ID_KEY, TEST_PASSWORD};
use rn_kernel::{feed::ClusterFeed, ids};

/// ≤ 2 ms cold, from the kernel report's performance gates.
const COLD_BUDGET: Duration = Duration::from_millis(2);
/// ≤ 50 µs warm.
const WARM_BUDGET: Duration = Duration::from_micros(50);
/// One intra-zone RTT plus a millisecond. On loopback the RTT is ~0, so what
/// is left is the millisecond and the fsync.
const COMMIT_BUDGET: Duration = Duration::from_millis(1);

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime starts")
}

/// Time an async closure by hand and report it against its budget.
///
/// Criterion's own report is in `target/criterion`; this line is what a person
/// running the suite actually reads.
fn report(name: &str, elapsed: Duration, iterations: u32, budget: Duration) {
    let each = elapsed / iterations;
    let verdict = if each <= budget { "within" } else { "OVER" };
    println!("  budget  {name}: {each:?} per op, {verdict} {budget:?}");
}

/// What one argon2id hash costs on this machine.
///
/// `Register` hashes a password, and argon2 is *deliberately* tens of
/// milliseconds — it would swamp everything else and make the commit budget
/// unreadable. The hash is not what raft changes, so it is measured once and
/// subtracted, and both numbers are printed rather than only the flattering
/// one.
fn hash_cost() -> Duration {
    let started = Instant::now();
    for _ in 0..8 {
        std::hint::black_box(rn_kernel::password::hash(TEST_PASSWORD).expect("hashes"));
    }
    started.elapsed() / 8
}

/// Report a command's commit cost with the hash taken out of it.
fn report_commit(name: &str, elapsed: Duration, iterations: u32, hash: Duration) {
    let each = elapsed / iterations;
    let commit = each.saturating_sub(hash);
    let verdict = if commit <= COMMIT_BUDGET {
        "within"
    } else {
        "OVER"
    };
    println!(
        "  budget  {name}: {each:?} per op, of which {hash:?} is argon2id; \
         commit {commit:?}, {verdict} {COMMIT_BUDGET:?}"
    );
}

fn ids(c: &mut Criterion) {
    let key = IdKey::from_hex(TEST_ID_KEY).expect("the test key is well formed");
    let id = Id::<Identity>::new(4_242);
    let public = id.public(&key);

    c.bench_function("id/encode", |b| {
        b.iter(|| std::hint::black_box(id.public(&key)));
    });
    c.bench_function("id/decode", |b| {
        b.iter(|| std::hint::black_box(ids::decode::<Identity>(&key, &public).expect("decodes")));
    });

    let started = Instant::now();
    for _ in 0..100_000 {
        std::hint::black_box(ids::decode::<Identity>(&key, &id.public(&key)).expect("decodes"));
    }
    report(
        "id round trip",
        started.elapsed(),
        100_000,
        Duration::from_micros(5),
    );
}

fn principal(c: &mut Criterion) {
    let rt = runtime();
    let harness = Local::new();
    let who = rt
        .block_on(harness.register("Ronit", "bench@example.test"))
        .expect("registers");
    let cache = PrincipalCache::new(4_096);
    rt.block_on(resolve_cached(harness.store(), &cache, &who.token))
        .expect("warms");

    c.bench_function("principal/cold", |b| {
        b.to_async(&rt).iter(|| async {
            resolve(harness.store(), &who.token)
                .await
                .expect("resolves")
        });
    });
    c.bench_function("principal/warm", |b| {
        b.to_async(&rt).iter(|| async {
            resolve_cached(harness.store(), &cache, &who.token)
                .await
                .expect("resolves")
        });
    });

    let started = Instant::now();
    rt.block_on(async {
        for _ in 0..2_000 {
            std::hint::black_box(
                resolve(harness.store(), &who.token)
                    .await
                    .expect("resolves"),
            );
        }
    });
    report("principal cold", started.elapsed(), 2_000, COLD_BUDGET);

    let started = Instant::now();
    rt.block_on(async {
        for _ in 0..20_000 {
            std::hint::black_box(
                resolve_cached(harness.store(), &cache, &who.token)
                    .await
                    .expect("resolves"),
            );
        }
    });
    report("principal warm", started.elapsed(), 20_000, WARM_BUDGET);
}

/// Arguments for one registration.
fn registration(n: u32) -> Register {
    Register {
        display_name: "Ronit".into(),
        handle: format!("bench-{n}"),
        email: format!("bench-{n}@example.test"),
        password: TEST_PASSWORD.into(),
    }
}

fn commit_in_process(c: &mut Criterion) {
    let rt = runtime();
    let harness = &Local::new();
    let mut n = 0u32;

    c.bench_function("commit/register/rusqlite", |b| {
        b.to_async(&rt).iter(|| {
            n += 1;
            let args = registration(n);
            async move {
                cmd::register(&harness.ctx(Principal::Anonymous), &args)
                    .await
                    .expect("registers")
            }
        });
    });

    let started = Instant::now();
    rt.block_on(async {
        for i in 0..200 {
            cmd::register(
                &harness.ctx(Principal::Anonymous),
                &registration(1_000_000 + i),
            )
            .await
            .expect("registers");
        }
    });
    report_commit("register (rusqlite)", started.elapsed(), 200, hash_cost());
}

fn commit_on_a_node(c: &mut Criterion) {
    let rt = runtime();
    let Ok(node) = rt.block_on(Node::start()) else {
        println!("  skipped: no hiqlite node could be started here");
        return;
    };
    // `ClusterFeed` spawns its listener, so it has to be built inside the
    // runtime it will live in.
    let feed = {
        let _guard = rt.enter();
        ClusterFeed::new(node.store.clone())
    };
    let store = node.store.clone();
    let mut n = 0u32;

    let provider = rn_kernel::oidc::Provider::dev();
    let ctx = |key: uuid::Uuid| cmd::Ctx {
        store: store.as_ref(),
        feed: &feed,
        provider: &provider,
        principal: Principal::Anonymous,
        key,
    };

    c.bench_function("commit/register/hiqlite", |b| {
        b.to_async(&rt).iter(|| {
            n += 1;
            let args = registration(n);
            async move {
                cmd::register(&ctx(uuid::Uuid::new_v4()), &args)
                    .await
                    .expect("registers")
            }
        });
    });

    let started = Instant::now();
    rt.block_on(async {
        for i in 0..200 {
            cmd::register(&ctx(uuid::Uuid::new_v4()), &registration(2_000_000 + i))
                .await
                .expect("registers");
        }
    });
    report_commit(
        "register (hiqlite, one node)",
        started.elapsed(),
        200,
        hash_cost(),
    );

    drop(feed);
    drop(store);
    drop(node);
}

criterion_group!(benches, ids, principal, commit_in_process, commit_on_a_node);
criterion_main!(benches);
