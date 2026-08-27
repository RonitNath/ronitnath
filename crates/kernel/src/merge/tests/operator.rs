//! The first-operator bootstrap, and the guard that keeps it the first.
//!
//! `bootstrap_operator` is the one grant in this crate with no actor and no
//! audit row, which is defensible exactly once — the act that creates an
//! operator out of a deployment that has none. Every operator after that is
//! `SetRole`, run by an operator who is on the record. So the interesting
//! behaviour is not the insert but the refusal.

use super::prelude::*;

/// Operator rows on `platform:*`, whoever holds them.
const OPERATORS: &str = "SELECT count(*) AS n FROM relation \
     WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator'";

/// Whether this person holds one.
const HOLDS: &str = "SELECT count(*) AS n FROM relation \
     WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator' \
       AND subject_kind = 'person' AND subject_id = $1";

#[tokio::test]
async fn a_deployment_with_no_operator_gets_its_first_one() {
    let harness = Local::new();
    let (_, person) = who(&harness
        .register("Ronit", "first-operator@example.test")
        .await
        .expect("registers"));
    assert_eq!(count(&harness, OPERATORS, bind![]).await, 0);

    crate::bootstrap_operator(harness.store(), person)
        .await
        .expect("an empty deployment accepts its first operator");

    assert_eq!(count(&harness, HOLDS, bind![person]).await, 1);
}

#[tokio::test]
async fn a_deployment_that_already_has_one_refuses_a_second() {
    let harness = Local::new();
    let (_, first) = who(&harness
        .register("Ronit", "operator-one@example.test")
        .await
        .expect("registers"));
    let (_, second) = who(&harness
        .register("Abeer", "operator-two@example.test")
        .await
        .expect("registers"));
    crate::bootstrap_operator(harness.store(), first)
        .await
        .expect("the first one is the bootstrap");

    // A second is a decline, not a quiet no-op: the caller is a CLI and the
    // person running it has to be told the deployment is past this point.
    let refused = crate::bootstrap_operator(harness.store(), second).await;
    assert!(
        matches!(refused, Err(KernelError::Decline(_))),
        "{refused:?}"
    );

    // And nothing was written — including for the person who already held it,
    // so re-running the bootstrap cannot be mistaken for idempotence.
    assert_eq!(count(&harness, OPERATORS, bind![]).await, 1);
    assert_eq!(count(&harness, HOLDS, bind![second]).await, 0);

    let again = crate::bootstrap_operator(harness.store(), first).await;
    assert!(matches!(again, Err(KernelError::Decline(_))), "{again:?}");
    assert_eq!(count(&harness, OPERATORS, bind![]).await, 1);
}
