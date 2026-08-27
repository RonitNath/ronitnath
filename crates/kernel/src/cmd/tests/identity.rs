//! `Register` and `SignIn`: creating a registration and proving one.

use super::prelude::*;

// -------------------------------------------------------------- register ---

#[tokio::test]
async fn register_creates_a_person_an_identity_two_factors_and_a_session() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "ronit@example.test")
        .await
        .expect("registers");

    assert_eq!(count(&harness, "SELECT count(*) AS n FROM party").await, 1);
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM identity").await,
        1
    );
    assert_eq!(count(&harness, "SELECT count(*) AS n FROM factor").await, 2);
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM session").await,
        1
    );
    assert!(matches!(who.principal, Principal::Member { .. }));
}

#[tokio::test]
async fn register_refuses_an_address_that_is_already_registered_the_way_it_refuses_everything() {
    let harness = Local::new();
    harness
        .register("Ronit", "taken@example.test")
        .await
        .expect("registers");
    let again = register(
        &harness.ctx(Principal::Anonymous),
        &registration("taken@example.test"),
    )
    .await;
    assert!(
        matches!(again, Err(ref err) if err.is_decline()),
        "a taken address is a decline, not a message saying it is taken: {again:?}"
    );
    assert_eq!(count(&harness, "SELECT count(*) AS n FROM party").await, 1);
}

#[tokio::test]
async fn register_validates_its_arguments_and_says_which_one() {
    let harness = Local::new();
    let ctx = harness.ctx(Principal::Anonymous);

    let mut args = registration("nope");
    assert!(matches!(
        register(&ctx, &args).await,
        Err(crate::KernelError::Invalid(Invalid::NotAnEmail))
    ));

    args = registration("short@example.test");
    args.password = "short".into();
    assert!(matches!(
        register(&ctx, &args).await,
        Err(crate::KernelError::Invalid(Invalid::PasswordTooShort(
            PASSWORD_MIN
        )))
    ));

    args = registration("blank@example.test");
    args.display_name = "   ".into();
    assert!(matches!(
        register(&ctx, &args).await,
        Err(crate::KernelError::Invalid(Invalid::Missing(
            "display name"
        )))
    ));
}

#[tokio::test]
async fn a_replayed_key_returns_the_original_and_a_reused_one_conflicts() {
    let harness = Local::new();
    let key = Uuid::new_v4();
    let args = registration("replay@example.test");

    let first = register(&harness.ctx_keyed(Principal::Anonymous, key), &args)
        .await
        .expect("registers");
    let again = register(&harness.ctx_keyed(Principal::Anonymous, key), &args)
        .await
        .expect("replays");

    assert_eq!(first.committed, again.committed, "the same answer");
    assert!(first.token.is_some(), "the mint returns its token once");
    assert!(
        again.token.is_none(),
        "and a replay cannot, because the row keeps only a digest"
    );
    assert_eq!(count(&harness, "SELECT count(*) AS n FROM party").await, 1);

    let different = register(
        &harness.ctx_keyed(Principal::Anonymous, key),
        &registration("someone-else@example.test"),
    )
    .await;
    assert!(matches!(different, Err(crate::KernelError::Conflict)));
}

// --------------------------------------------------------------- sign in ---

#[tokio::test]
async fn sign_in_mints_a_session_for_the_right_password_and_nothing_else() {
    let harness = Local::new();
    harness
        .register("Ronit", "in@example.test")
        .await
        .expect("registers");

    let ctx = harness.ctx(Principal::Anonymous);
    let wrong = sign_in(
        &ctx,
        &SignIn {
            email: "in@example.test".into(),
            password: "not the password".into(),
        },
    )
    .await;
    assert!(matches!(wrong, Err(ref e) if e.is_decline()));

    let unknown = sign_in(
        &ctx,
        &SignIn {
            email: "nobody@example.test".into(),
            password: TEST_PASSWORD.into(),
        },
    )
    .await;
    assert!(matches!(unknown, Err(ref e) if e.is_decline()));
    assert_eq!(
        format!("{:?}", wrong.unwrap_err()),
        format!("{:?}", unknown.unwrap_err()),
        "a wrong password and an unknown address are the same answer"
    );

    let good = harness.sign_in("in@example.test").await.expect("signs in");
    assert!(matches!(good.principal, Principal::Member { .. }));
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM session").await,
        2
    );
}

#[tokio::test]
async fn sign_in_is_case_insensitive_about_the_address() {
    let harness = Local::new();
    harness
        .register("Ronit", "Mixed.Case@Example.Test")
        .await
        .expect("registers");
    harness
        .sign_in("mixed.case@example.test")
        .await
        .expect("one human, one address");
}

#[tokio::test]
async fn a_disabled_identity_cannot_sign_in() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "off@example.test")
        .await
        .expect("registers");
    harness
        .store()
        .execute("UPDATE identity SET status = 'disabled'", bind![])
        .await
        .expect("update runs");

    let refused = sign_in(
        &harness.ctx(Principal::Anonymous),
        &SignIn {
            email: "off@example.test".into(),
            password: TEST_PASSWORD.into(),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));

    // And the session it already had stops resolving.
    let resolved = crate::principal::resolve(harness.store(), &who.token)
        .await
        .expect("resolves");
    assert_eq!(resolved.principal, Principal::Anonymous);
}
