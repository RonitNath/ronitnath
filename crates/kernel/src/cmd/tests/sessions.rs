//! Sessions, and the audit row every command leaves behind.

use super::prelude::*;

// ---------------------------------------------------------- sessions out ---

#[tokio::test]
async fn sign_out_ends_the_session_it_arrived_on_and_no_other() {
    let harness = Local::new();
    let first = harness
        .register("Ronit", "out@example.test")
        .await
        .expect("registers");
    let second = harness.sign_in("out@example.test").await.expect("signs in");

    sign_out(&harness.ctx(first.principal.clone()), &SignOut {})
        .await
        .expect("signs out");

    assert_eq!(
        crate::principal::resolve(harness.store(), &first.token)
            .await
            .expect("resolves")
            .principal,
        Principal::Anonymous
    );
    assert!(matches!(
        crate::principal::resolve(harness.store(), &second.token)
            .await
            .expect("resolves")
            .principal,
        Principal::Member { .. }
    ));
}

#[tokio::test]
async fn a_session_belonging_to_somebody_else_cannot_be_revoked() {
    let harness = Local::new();
    let mine = harness
        .register("Ronit", "mine@example.test")
        .await
        .expect("registers");
    let theirs = harness
        .register("Someone", "theirs@example.test")
        .await
        .expect("registers");

    let their_session = theirs
        .principal
        .session()
        .expect("a member has a session")
        .public(harness.store().ids());

    let refused = revoke_session(
        &harness.ctx(mine.principal.clone()),
        &RevokeSession {
            session: their_session,
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM session").await,
        2
    );

    // My own, on the other hand.
    let second = harness
        .sign_in("mine@example.test")
        .await
        .expect("signs in");
    let committed = revoke_session(
        &harness.ctx(mine.principal.clone()),
        &RevokeSession {
            session: second
                .principal
                .session()
                .expect("has one")
                .public(harness.store().ids()),
        },
    )
    .await
    .expect("revokes");
    assert!(matches!(committed.event, Event::SessionRevoked { .. }));
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM session").await,
        2,
        "my second session is gone; my first and theirs are not"
    );
}

#[tokio::test]
async fn a_forged_session_id_is_refused_without_a_query() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "forge@example.test")
        .await
        .expect("registers");
    let forged = format!("s_{}", "A".repeat(22))
        .parse()
        .expect("well-formed shape");
    let refused = revoke_session(
        &harness.ctx(who.principal),
        &RevokeSession { session: forged },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));
}

#[tokio::test]
async fn an_anonymous_principal_cannot_run_a_member_command() {
    let harness = Local::new();
    let ctx = harness.ctx(Principal::Anonymous);
    assert!(sign_out(&ctx, &SignOut {}).await.unwrap_err().is_decline());
    assert!(
        add_factor(
            &ctx,
            &AddFactor {
                kind: FactorKind::Email,
                value: "x@example.test".into(),
            },
        )
        .await
        .unwrap_err()
        .is_decline()
    );
}

// ------------------------------------------------------------- the audit ---

#[tokio::test]
async fn every_command_writes_exactly_one_audit_row_inside_its_own_transaction() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "audited@example.test")
        .await
        .expect("registers");
    sign_out(&harness.ctx(who.principal), &SignOut {})
        .await
        .expect("signs out");
    let second = harness
        .sign_in("audited@example.test")
        .await
        .expect("signs in");
    add_factor(
        &harness.ctx(second.principal),
        &AddFactor {
            kind: FactorKind::Email,
            value: "more@example.test".into(),
        },
    )
    .await
    .expect("adds");

    let rows: Vec<crate::audit::Recorded> = harness
        .store()
        .query(
            "SELECT id, request_digest, payload FROM audit ORDER BY id",
            bind![],
        )
        .await
        .expect("query runs");
    let names: Vec<&'static str> = rows
        .iter()
        .filter_map(|r| r.event.as_ref().map(Event::name))
        .collect();
    assert_eq!(names, ["register", "sign-out", "sign-in", "add-factor"]);
    assert_eq!(
        rows.iter().map(|r| r.offset).collect::<Vec<_>>(),
        [1, 2, 3, 4],
        "the audit id is the offset, and it is dense and ordered"
    );
}

#[tokio::test]
async fn a_session_that_ran_out_of_time_resolves_to_nobody() {
    let harness = Local::new();
    let who = harness
        .register("Ronit", "expiry@example.test")
        .await
        .expect("registers");
    harness.advance(SESSION_TTL + 1);
    assert_eq!(
        crate::principal::resolve(harness.store(), &who.token)
            .await
            .expect("resolves")
            .principal,
        Principal::Anonymous
    );
    assert_eq!(
        crate::observe::sweep_sessions(harness.store())
            .await
            .expect("sweeps"),
        1
    );
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM session").await,
        0
    );
}
