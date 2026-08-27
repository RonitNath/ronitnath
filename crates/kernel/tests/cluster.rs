//! One flow against a real hiqlite node: register, sign in, hear the feed wake
//! up, sign out.
//!
//! Everything else in this crate runs against the in-process engine, which is
//! the same SQLite and proves the same SQL. Three things it cannot prove, and
//! this test exists for exactly those:
//!
//! * a batch survives being serialised through the raft log — in particular
//!   that `Param::StmtOutput`, the reference from one statement to an earlier
//!   one's `RETURNING id`, means the same thing on the leader as it does
//!   in-process. Registration is six statements wired together that way, so if
//!   it disagreed anywhere this is where it shows;
//! * the migrations apply through `Client::migrate`, hash and all, rather than
//!   through `execute_batch`;
//! * `notify` on the cache group actually wakes a subscriber.

use futures_util::StreamExt;
use rn_api::commands::SignOut;
use rn_kernel::bind;
use rn_kernel::cmd::{self, Ctx};
use rn_kernel::event::Event;
use rn_kernel::feed::{ClusterFeed, Feed};
use rn_kernel::principal::{Principal, resolve};
use rn_kernel::store::{Count, Reads};
use rn_kernel::testing::{Node, TEST_PASSWORD};

#[tokio::test(flavor = "multi_thread")]
async fn a_registration_flows_through_raft_and_out_of_the_feed() {
    let node = Node::start().await.expect("a single node starts");
    let path = node.path();
    let feed = ClusterFeed::new(node.store.clone());
    let store = node.store.clone();
    let mut wakes = Box::pin(feed.subscribe());

    let ctx = |principal: Principal| Ctx {
        store: store.as_ref(),
        feed: &feed,
        principal,
        key: uuid::Uuid::new_v4(),
    };

    // Register: six statements in one raft-committed batch, wired together by
    // statement-output references.
    let registered = cmd::register(
        &ctx(Principal::Anonymous),
        &rn_api::commands::Register {
            display_name: "Ronit".into(),
            email: "cluster@example.test".into(),
            password: TEST_PASSWORD.into(),
        },
    )
    .await
    .expect("registers");
    let token = registered.token.expect("a fresh mint returns its token");

    let rows: Vec<Count> = store
        .query("SELECT count(*) AS n FROM factor", bind![])
        .await
        .expect("query runs");
    assert_eq!(
        rows[0].0, 2,
        "both factors point at the identity the batch made"
    );

    let resolved = resolve(store.as_ref(), &token).await.expect("resolves");
    let Principal::Member { identity, .. } = resolved.principal.clone() else {
        panic!("the session the batch minted must resolve");
    };

    // Sign in on a second device.
    let signed_in = cmd::sign_in(
        &ctx(Principal::Anonymous),
        &rn_api::commands::SignIn {
            email: "cluster@example.test".into(),
            password: TEST_PASSWORD.into(),
        },
    )
    .await
    .expect("signs in");
    assert!(matches!(signed_in.committed.event, Event::SignedIn { .. }));

    // The wake-up crossed the cache raft group and came back.
    let offset = tokio::time::timeout(std::time::Duration::from_secs(10), wakes.next())
        .await
        .expect("a commit wakes a subscriber over listen/notify")
        .expect("the stream is live");
    assert!(offset >= registered.committed.offset);

    // Sign out, and read the whole story back off the durable feed.
    cmd::sign_out(&ctx(resolved.principal), &SignOut {})
        .await
        .expect("signs out");

    let events = feed.read(0, 16).await.expect("reads");
    assert_eq!(
        events.iter().map(|c| c.event.name()).collect::<Vec<_>>(),
        ["register", "sign-in", "sign-out"]
    );
    assert_eq!(
        events[0].event.touches_identity(),
        Some(identity),
        "the payload names the identity the batch allocated"
    );
    assert_eq!(
        events.iter().map(|c| c.offset).collect::<Vec<_>>(),
        [1, 2, 3],
        "audit.id is the offset, dense and ordered, through raft too"
    );

    let sessions: Vec<Count> = store
        .query("SELECT count(*) AS n FROM session", bind![])
        .await
        .expect("query runs");
    assert_eq!(sessions[0].0, 1, "one device signed out, one still in");

    // Leave nothing behind: dropping the node takes its data directory with
    // it, which is what stops the next run of this test finding somebody
    // else's raft state on disk.
    drop(feed);
    drop(store);
    drop(node);
    assert!(
        !path.exists(),
        "the node's data directory outlived the node"
    );
}
