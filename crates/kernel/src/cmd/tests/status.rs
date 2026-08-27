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

// ------------------------------------------------ the other party kinds ---

#[tokio::test]
async fn an_organization_and_its_groups_are_disabled_by_their_owner_and_by_nobody_else() {
    use super::world::{self, key, public};

    let harness = Local::new();
    let owner = world::person(&harness, "Owner", "org-owner@example.test").await;
    let stranger = world::person(&harness, "Stranger", "org-stranger@example.test").await;
    let (org, _) = world::organization(&harness, &owner, "Isoastra")
        .await
        .expect("founds");
    let team = world::group(&harness, &owner, "Team", Some(org))
        .await
        .expect("creates");
    let organization = public(key(&harness), org);
    let group = public(key(&harness), team);

    // A person who is nothing to it is refused, and refused the same way a
    // forged id would be.
    for party in [organization.clone(), group.clone()] {
        let refused = disable(
            &harness.ctx(stranger.principal.clone()),
            &Disable {
                party,
                reason: "not mine".into(),
            },
        )
        .await;
        assert!(matches!(refused, Err(ref e) if e.is_decline()));
    }

    // The owner may, and the event names the party that moved.
    let committed = disable(
        &harness.ctx(owner.principal.clone()),
        &Disable {
            party: organization.clone(),
            reason: "wound up".into(),
        },
    )
    .await
    .expect("its owner may");
    assert_eq!(
        committed.event,
        Event::PartyDisabled {
            party: ids::Id::new(org.get())
        }
    );

    // A group of that organization goes the same way, through the
    // organization's ownership rather than through a membership of its own.
    disable(
        &harness.ctx(owner.principal.clone()),
        &Disable {
            party: group.clone(),
            reason: "wound up".into(),
        },
    )
    .await
    .expect("the owning organization's owner may");
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM party WHERE status = 'disabled'"
        )
        .await,
        2
    );

    // Disabling an organization signs nobody out: sessions belong to
    // identities, and an organization has none.
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM session").await,
        2,
        "disabling an organization ended somebody's session"
    );

    enable(
        &harness.ctx(owner.principal.clone()),
        &Enable { party: group },
    )
    .await
    .expect("and may put it back");
    enable(
        &harness.ctx(owner.principal),
        &Enable {
            party: organization,
        },
    )
    .await
    .expect("and may put it back");
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM party WHERE status = 'disabled'"
        )
        .await,
        0
    );
}

#[tokio::test]
async fn a_platform_operator_may_disable_an_organization_they_have_nothing_to_do_with() {
    use super::world::{self, key, public};

    let harness = Local::new();
    let owner = world::person(&harness, "Owner", "op-owner@example.test").await;
    let operator = world::person(&harness, "Operator", "op-operator@example.test").await;
    let (org, _) = world::organization(&harness, &owner, "Isoastra")
        .await
        .expect("founds");
    let party = public(key(&harness), org);

    let refused = disable(
        &harness.ctx(operator.principal.clone()),
        &Disable {
            party: party.clone(),
            reason: "ruling".into(),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));

    make_operator(&harness, operator.person()).await;
    disable(
        &harness.ctx(operator.principal),
        &Disable {
            party,
            reason: "ruling".into(),
        },
    )
    .await
    .expect("an operator may");
}
