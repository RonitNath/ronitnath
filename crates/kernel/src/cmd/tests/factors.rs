//! Adding, removing and proving factors.

use super::prelude::*;

// ---------------------------------------------------------------- factors ---

#[tokio::test]
async fn a_factor_can_be_added_and_the_unbuilt_kinds_are_refused() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "factors@example.test")
        .await
        .expect("registers");
    let ctx = harness.ctx(who.principal.clone());

    let committed = add_factor(
        &ctx,
        &AddFactor {
            kind: FactorKind::Email,
            value: "second@example.test".into(),
        },
    )
    .await
    .expect("adds");
    assert!(matches!(
        committed.event,
        Event::FactorAdded {
            kind: crate::domain::FactorKind::Email,
            ..
        }
    ));

    let refused = add_factor(
        &harness.ctx(who.principal),
        &AddFactor {
            kind: FactorKind::Passkey,
            value: "credential".into(),
        },
    )
    .await;
    assert!(matches!(
        refused,
        Err(crate::KernelError::Invalid(Invalid::UnsupportedFactor))
    ));
}

#[tokio::test]
async fn the_last_factor_of_a_kind_cannot_be_removed() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "last@example.test")
        .await
        .expect("registers");

    let only_email: Vec<crate::store::RowId<ids::Factor>> = harness
        .store()
        .query("SELECT id FROM factor WHERE kind = 'email'", bind![])
        .await
        .expect("query runs");
    let target = only_email[0].0.public(harness.store().ids());

    let refused = remove_factor(
        &harness.ctx(who.principal.clone()),
        &RemoveFactor {
            factor: target.clone(),
        },
    )
    .await;
    assert!(
        matches!(refused, Err(ref e) if e.is_decline()),
        "an identity never loses its last way in"
    );

    // Add a second, and now the first can go.
    add_factor(
        &harness.ctx(who.principal.clone()),
        &AddFactor {
            kind: FactorKind::Email,
            value: "spare@example.test".into(),
        },
    )
    .await
    .expect("adds");
    remove_factor(
        &harness.ctx(who.principal),
        &RemoveFactor { factor: target },
    )
    .await
    .expect("removes");
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM factor WHERE kind = 'email'"
        )
        .await,
        1
    );
}

// ----------------------------------------------------------- verify email ---

#[tokio::test]
async fn a_verification_token_proves_an_address_once() {
    let harness = Local::new();
    harness
        .register("Ronit", "verify@example.test")
        .await
        .expect("registers");
    let factor: Vec<crate::store::RowId<ids::Factor>> = harness
        .store()
        .query("SELECT id FROM factor WHERE kind = 'email'", bind![])
        .await
        .expect("query runs");
    let token = mint_verification(harness.store(), factor[0].0)
        .await
        .expect("mints");

    // Anonymous on purpose: the person is in their mail client, not the app.
    let committed = verify_email(
        &harness.ctx(Principal::Anonymous),
        &VerifyEmail {
            token: token.expose().to_owned(),
        },
    )
    .await
    .expect("verifies");
    assert!(matches!(committed.event, Event::EmailVerified { .. }));
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM factor WHERE verified_at IS NOT NULL"
        )
        .await,
        1
    );

    let again = verify_email(
        &harness.ctx(Principal::Anonymous),
        &VerifyEmail {
            token: token.expose().to_owned(),
        },
    )
    .await;
    assert!(
        matches!(again, Err(ref e) if e.is_decline()),
        "claimed once"
    );
}

#[tokio::test]
async fn an_expired_verification_token_does_not_work() {
    let harness = Local::new();
    harness
        .register("Ronit", "stale@example.test")
        .await
        .expect("registers");
    let factor: Vec<crate::store::RowId<ids::Factor>> = harness
        .store()
        .query("SELECT id FROM factor WHERE kind = 'email'", bind![])
        .await
        .expect("query runs");
    let token = mint_verification(harness.store(), factor[0].0)
        .await
        .expect("mints");
    harness.advance(crate::domain::VERIFY_TTL + 1);

    let refused = verify_email(
        &harness.ctx(Principal::Anonymous),
        &VerifyEmail {
            token: token.expose().to_owned(),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));
}
