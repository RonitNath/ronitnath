//! The scan: what the observation lane proposes, and what it may never write.

use super::prelude::*;
use crate::observe::Matches;

/// The signal and score of every candidate in the table, ordered.
async fn queued(harness: &Local) -> Vec<(String, String)> {
    #[derive(Debug)]
    struct Row(String, String);
    impl crate::store::FromRow for Row {
        fn from_row(row: &mut impl crate::store::Cursor) -> Result<Self, crate::store::RowError> {
            Ok(Self(row.text("signal")?, row.text("status")?))
        }
    }
    harness
        .store()
        .query::<Row>(
            "SELECT signal, status FROM match_candidate ORDER BY signal",
            bind![],
        )
        .await
        .expect("query runs")
        .into_iter()
        .map(|row| (row.0, row.1))
        .collect()
}

#[tokio::test]
async fn a_shared_verified_subject_proposes_the_pair() {
    let harness = Local::new();
    let first = harness
        .register("Ronit", "scan-a@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit N", "scan-b@example.test")
        .await
        .expect("registers");
    shared_subject(&harness, who(&first).0, "google|1234").await;
    shared_subject(&harness, who(&second).0, "google|1234").await;

    let written = scan_identity(harness.store(), who(&first).0)
        .await
        .expect("the scan runs");
    assert_eq!(written, 1);
    assert_eq!(
        queued(&harness).await,
        vec![("oidc_subject".to_owned(), "proposed".to_owned())]
    );

    // Scanning from the other side is the same pair, not a second one.
    scan_identity(harness.store(), who(&second).0)
        .await
        .expect("the scan runs");
    assert_eq!(queued(&harness).await.len(), 1);
}

#[tokio::test]
async fn claiming_a_link_another_identity_minted_proposes_the_pair() {
    let harness = Local::new();
    let first = harness
        .register("Ronit", "link-a@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit", "link-b@example.test")
        .await
        .expect("registers");

    // A link that verifies the *first* identity's address, claimed by somebody
    // holding the *second*. K1's `VerifyEmail` records the factor's own
    // identity as the claimer — the link it mints is anonymous by design, since
    // the person clicking a mail link is often signed out — so the row this
    // signal reads is the one K2's invitation `ClaimLink` writes. The signal is
    // about that row, so that row is what the test writes.
    let factor: Vec<Count> = harness
        .store()
        .query(
            "SELECT id AS n FROM factor WHERE identity_id = $1 AND kind = 'email'",
            bind![who(&first).0],
        )
        .await
        .expect("query runs");
    write(
        harness.store(),
        "INSERT INTO link \
         (token_hash, expires_at, claimed_by_identity_id, claimed_at, verifies_factor_id, \
          created_at) \
         VALUES ($1, $2, $3, $4, $5, $4)",
        bind![
            vec![7u8; 32],
            harness.store().now() + 3_600,
            who(&second).0,
            harness.store().now(),
            factor[0].0
        ],
    )
    .await;

    let written = scan_identity(harness.store(), who(&second).0)
        .await
        .expect("the scan runs");
    assert_eq!(written, 1);
    assert_eq!(
        queued(&harness).await,
        vec![("claimed_link".to_owned(), "proposed".to_owned())]
    );
}

#[tokio::test]
async fn a_shared_name_in_a_shared_group_is_the_weakest_proposal() {
    let harness = Local::new();
    let first = harness
        .register("Ronit", "name-a@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit", "name-b@example.test")
        .await
        .expect("registers");
    let reviewers = group(&harness, "reviewers").await;
    join(&harness, reviewers, who(&first).1, "member").await;
    join(&harness, reviewers, who(&second).1, "member").await;

    scan_identity(harness.store(), who(&first).0)
        .await
        .expect("the scan runs");
    assert_eq!(
        queued(&harness).await,
        vec![("name_and_group".to_owned(), "proposed".to_owned())]
    );
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM match_candidate WHERE score < 0.5",
            bind![],
        )
        .await,
        1,
        "the same name in the same group is a Tuesday, not evidence"
    );
}

#[tokio::test]
async fn the_same_name_without_a_shared_group_proposes_nothing() {
    let harness = Local::new();
    let first = harness
        .register("Ronit", "alone-a@example.test")
        .await
        .expect("registers");
    harness
        .register("Ronit", "alone-b@example.test")
        .await
        .expect("registers");

    assert_eq!(
        scan_identity(harness.store(), who(&first).0)
            .await
            .expect("the scan runs"),
        0
    );
    assert!(queued(&harness).await.is_empty());
}

#[tokio::test]
async fn a_scan_never_proposes_a_pair_that_is_already_one_person() {
    let harness = Local::new();
    let first = harness
        .register("Ronit", "one-a@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit", "one-b@example.test")
        .await
        .expect("registers");
    shared_subject(&harness, who(&first).0, "sub-same").await;
    shared_subject(&harness, who(&second).0, "sub-same").await;
    let candidate = queue(
        &harness,
        who(&first).0,
        who(&second).0,
        MatchSignal::OidcSubject,
    )
    .await;
    confirm_match(
        &harness.ctx(first.principal),
        &ConfirmMatch {
            candidate: public(&harness, candidate),
            other_session: None,
        },
    )
    .await
    .expect("a shared verified factor merges them");

    assert_eq!(
        scan_identity(harness.store(), who(&second).0)
            .await
            .expect("the scan runs"),
        0,
        "there is no question left to ask"
    );
}

#[tokio::test]
async fn the_drain_scans_what_the_three_proving_events_queued() {
    let harness = Local::new();
    let first = harness
        .register("Ronit", "drain-a@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit", "drain-b@example.test")
        .await
        .expect("registers");
    shared_subject(&harness, who(&first).0, "drain-sub").await;
    shared_subject(&harness, who(&second).0, "drain-sub").await;

    let queue = Matches::new();
    assert!(queue.note(who(&first).0));
    assert_eq!(queue.drain(harness.store()).await.expect("drains"), 1);
    assert!(queue.is_empty());
    assert_eq!(queued(&harness).await.len(), 1);

    // Idempotent: the scan's only statement is an upsert on the pair, so
    // draining again means the same thing and adds no rows.
    assert!(queue.note(who(&second).0));
    assert_eq!(queue.drain(harness.store()).await.expect("drains"), 1);
    assert_eq!(queued(&harness).await.len(), 1);
}

#[tokio::test]
async fn no_signal_path_can_write_anything_but_proposed() {
    let harness = Local::new();
    let first = harness
        .register("Ronit", "never-a@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit", "never-b@example.test")
        .await
        .expect("registers");
    shared_subject(&harness, who(&first).0, "never").await;
    shared_subject(&harness, who(&second).0, "never").await;
    let reviewers = group(&harness, "reviewers").await;
    join(&harness, reviewers, who(&first).1, "member").await;
    join(&harness, reviewers, who(&second).1, "member").await;

    let (a, b) = (who(&first).0, who(&second).0);
    // Every signal about this pair, raised by every route a signal has.
    scan_identity(harness.store(), who(&first).0)
        .await
        .expect("the scan runs");
    scan_identity(harness.store(), who(&second).0)
        .await
        .expect("the scan runs");
    propose_match(
        &harness.ctx(first.principal),
        &ProposeMatch {
            identity_a: public(&harness, a),
            identity_b: public(&harness, b),
            signal: MatchSignal::VerifiedEmail,
        },
    )
    .await
    .expect("a person may raise their own");

    assert!(
        queued(&harness)
            .await
            .iter()
            .all(|(_, status)| status == "proposed"),
        "signals propose; only proof disposes"
    );
    assert_eq!(person(&harness, who(&second).0).await, who(&second).1);
}
