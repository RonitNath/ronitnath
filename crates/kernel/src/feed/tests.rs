//! The change feed: what it carries, and what a consumer resumes from.

use futures_util::StreamExt;

use super::*;
use crate::event::Event;
use crate::testing::Local;

#[tokio::test]
async fn the_feed_is_the_audit_table_in_order() {
    let harness = Local::new();
    harness
        .register("Ronit", "feed@example.test")
        .await
        .expect("registers");
    harness
        .sign_in("feed@example.test")
        .await
        .expect("signs in");

    let events = harness.feed.read(0, 16).await.expect("reads");
    assert_eq!(
        events.iter().map(|c| c.event.name()).collect::<Vec<_>>(),
        ["register", "sign-in"]
    );
    assert_eq!(events[0].offset, 1);
    assert_eq!(events[1].offset, 2);
}

#[tokio::test]
async fn a_consumer_resumes_from_its_own_offset() {
    let harness = Local::new();
    harness
        .register("Ronit", "resume@example.test")
        .await
        .expect("registers");
    harness
        .sign_in("resume@example.test")
        .await
        .expect("signs in");

    let first = harness.feed.read(0, 1).await.expect("reads");
    assert_eq!(first.len(), 1, "the limit is honoured");
    let rest = harness.feed.read(first[0].offset, 16).await.expect("reads");
    assert_eq!(rest.len(), 1);
    assert!(matches!(rest[0].event, Event::SignedIn { .. }));
    assert!(
        harness
            .feed
            .read(rest[0].offset, 16)
            .await
            .expect("reads")
            .is_empty(),
        "and the end of the feed is empty, not an error"
    );
}

#[tokio::test]
async fn a_read_never_hands_back_more_than_the_limit_allows() {
    let harness = Local::new();
    harness
        .register("Ronit", "limit@example.test")
        .await
        .expect("registers");
    let asked_for_too_many = harness.feed.read(0, usize::MAX).await.expect("reads");
    assert!(asked_for_too_many.len() <= READ_LIMIT);
}

#[tokio::test]
async fn a_commit_wakes_a_subscriber_with_the_offset_it_should_read_to() {
    let harness = Local::new();
    // A stream built from an async block is not `Unpin`; a consumer pins it
    // once and polls it forever, which is what the server does too.
    let mut wakes = Box::pin(harness.feed.subscribe());

    let who = harness
        .register("Ronit", "wake@example.test")
        .await
        .expect("registers");
    let offset = tokio::time::timeout(std::time::Duration::from_secs(5), wakes.next())
        .await
        .expect("a commit wakes its subscribers")
        .expect("the stream is live");
    assert_eq!(offset, who.offset);

    let events = harness.feed.read(offset - 1, 16).await.expect("reads");
    assert!(matches!(events[0].event, Event::Registered { .. }));
}

#[tokio::test]
async fn a_wake_up_that_nobody_is_waiting_for_is_not_an_error() {
    let harness = Local::new();
    harness.feed.notify(42).await;
}
