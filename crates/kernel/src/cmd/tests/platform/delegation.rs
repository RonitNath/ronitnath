//! A2 — delegating operator authority, and taking it back.

use rn_api::commands::{GrantOperator, RevokeOperator};

use super::super::prelude::*;
use super::super::world::{self, key, public};
use super::count_of;
use crate::authority;
use crate::cmd;

#[tokio::test]
async fn an_operator_delegates_the_relation_and_takes_it_back() {
    let harness = Local::new();
    let a = world::person(&harness, "A", "d-a@example.test").await;
    let b = world::person(&harness, "B", "d-b@example.test").await;
    make_operator(&harness, a.person()).await;

    let grant = GrantOperator {
        person: public(key(&harness), b.person()),
        reason: "covering the cutover weekend".into(),
    };

    // B cannot grant it to themselves, which is the whole of why this is a
    // command with an actor rather than a second bootstrap.
    let refused = cmd::grant_operator(&harness.ctx(b.principal.clone()), &grant).await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));
    assert!(
        !authority::is_operator(harness.store(), b.person())
            .await
            .unwrap()
    );

    let committed = cmd::grant_operator(&harness.ctx(a.principal.clone()), &grant)
        .await
        .expect("an operator delegates");
    assert_eq!(
        committed.event,
        Event::OperatorGranted { person: b.person() }
    );
    assert!(
        authority::is_operator(harness.store(), b.person())
            .await
            .unwrap()
    );

    // The row records who granted it — the registration that authenticated,
    // which is the id every other writer of `relation.granted_by` puts there
    // and the one a display name resolves through.
    let granted_by = count(
        &harness,
        "SELECT count(*) AS n FROM relation \
         WHERE object_kind = 'platform' AND relation = 'operator' AND granted_by IS NOT NULL",
    )
    .await;
    assert_eq!(
        granted_by, 1,
        "the bootstrap's row has none and this one does"
    );

    // The audit row carries the reason. A delegation of god mode that did not
    // is a row nobody can account for.
    let reasoned = count(
        &harness,
        "SELECT count(*) AS n FROM audit \
         WHERE command = 'grant-operator' \
           AND json_extract(payload, '$.reason') = 'covering the cutover weekend'",
    )
    .await;
    assert_eq!(reasoned, 1);

    // And an `audit_object` row naming the person it was about, which is what
    // lets B find it without a scan of the deployment's whole history.
    let objects = count_of(
        &harness,
        "SELECT count(*) AS n FROM audit_object WHERE kind = 'party' AND id = $1",
        bind![b.person()],
    )
    .await;
    assert_eq!(objects, 1);

    // Taking it back.
    let revoke = RevokeOperator {
        person: public(key(&harness), b.person()),
        reason: "the cutover is done".into(),
    };
    let committed = cmd::revoke_operator(&harness.ctx(a.principal.clone()), &revoke)
        .await
        .expect("an operator revokes");
    assert_eq!(
        committed.event,
        Event::OperatorRevoked { person: b.person() }
    );
    assert!(
        !authority::is_operator(harness.store(), b.person())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn the_last_operator_cannot_be_revoked_and_the_deployment_still_has_one() {
    let harness = Local::new();
    let a = world::person(&harness, "A", "last-a@example.test").await;
    make_operator(&harness, a.person()).await;

    let revoke = RevokeOperator {
        person: public(key(&harness), a.person()),
        reason: "stepping down".into(),
    };
    let refused = cmd::revoke_operator(&harness.ctx(a.principal.clone()), &revoke).await;
    assert!(
        matches!(refused, Err(ref e) if e.is_decline()),
        "a deployment with no operator is one nobody can give the first to again"
    );
    assert_eq!(authority::operator_count(harness.store()).await.unwrap(), 1);

    // With a second one it is allowed, including on yourself.
    let b = world::person(&harness, "B", "last-b@example.test").await;
    cmd::grant_operator(
        &harness.ctx(a.principal.clone()),
        &GrantOperator {
            person: public(key(&harness), b.person()),
            reason: "so somebody else can hold it".into(),
        },
    )
    .await
    .expect("delegates");
    cmd::revoke_operator(&harness.ctx(a.principal.clone()), &revoke)
        .await
        .expect("revoking yourself is allowed while another operator exists");
    assert!(
        !authority::is_operator(harness.store(), a.person())
            .await
            .unwrap()
    );
    assert_eq!(authority::operator_count(harness.store()).await.unwrap(), 1);
}
