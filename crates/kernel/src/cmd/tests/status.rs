//! `Disable` and `Enable`: the party lifecycle and who may drive it.

use super::prelude::*;

// ------------------------------------------------------ disable / enable ---

#[tokio::test]
async fn a_person_can_disable_themselves_and_it_takes_their_sessions_with_it() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "self@example.test")
        .await
        .expect("registers");
    let party = who
        .principal
        .acting_as()
        .expect("a member acts as somebody")
        .public(harness.store().ids());

    let committed = disable(
        &harness.ctx(who.principal.clone()),
        &Disable {
            party,
            reason: "leaving".into(),
        },
    )
    .await
    .expect("disables");
    assert!(matches!(committed.event, Event::PartyDisabled { .. }));
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM session").await,
        0
    );
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM party WHERE status = 'disabled'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn one_person_cannot_disable_another_unless_they_are_a_platform_operator() {
    let harness = Local::new();
    let operator = harness
        .register("Operator", "operator@example.test")
        .await
        .expect("registers");
    let subject = harness
        .register("Subject", "subject@example.test")
        .await
        .expect("registers");
    let target = subject
        .principal
        .acting_as()
        .expect("acts as somebody")
        .public(harness.store().ids());

    let refused = disable(
        &harness.ctx(operator.principal.clone()),
        &Disable {
            party: target.clone(),
            reason: "because".into(),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));

    // Platform administration is a relation row, not a column.
    harness
        .store()
        .execute(
            "INSERT INTO relation \
             (object_kind, object_id, relation, subject_kind, subject_id, at) \
             VALUES ('platform', 0, 'operator', 'person', $1, $2)",
            bind![
                operator.principal.acting_as().expect("acts as somebody"),
                crate::testing::TEST_EPOCH
            ],
        )
        .await
        .expect("insert runs");

    disable(
        &harness.ctx(operator.principal.clone()),
        &Disable {
            party: target.clone(),
            reason: "ruling".into(),
        },
    )
    .await
    .expect("an operator may");

    enable(&harness.ctx(operator.principal), &Enable { party: target })
        .await
        .expect("and may put it back");
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM party WHERE status = 'active'"
        )
        .await,
        2
    );
}

#[tokio::test]
async fn disabling_a_party_that_is_already_disabled_declines_rather_than_pretending() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "twice@example.test")
        .await
        .expect("registers");
    let party = who
        .principal
        .acting_as()
        .expect("acts as somebody")
        .public(harness.store().ids());

    disable(
        &harness.ctx(who.principal.clone()),
        &Disable {
            party: party.clone(),
            reason: "first".into(),
        },
    )
    .await
    .expect("disables");

    let again = disable(
        &harness.ctx(who.principal),
        &Disable {
            party,
            reason: "second".into(),
        },
    )
    .await;
    assert!(matches!(again, Err(ref e) if e.is_decline()));
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM audit WHERE command = 'disable'"
        )
        .await,
        1,
        "and leaves no audit row for the change that did not happen"
    );
}
