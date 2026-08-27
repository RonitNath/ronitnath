//! What counts as proof, who may rule, and how a merge is undone.

use super::prelude::*;

/// Two registrations and a queued pair, with no proof of anything yet.
async fn queued(harness: &Local) -> (Registered, Registered, Id<MatchCandidate>) {
    let first = harness
        .register("Ronit", "one@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit", "two@example.test")
        .await
        .expect("registers");
    let candidate = queue(
        harness,
        who(&first).0,
        who(&second).0,
        MatchSignal::NameAndGroup,
    )
    .await;
    (first, second, candidate)
}

fn confirmation(
    harness: &Local,
    candidate: Id<MatchCandidate>,
    token: Option<&str>,
) -> ConfirmMatch {
    ConfirmMatch {
        candidate: public(harness, candidate),
        other_session: token.map(str::to_owned),
        other_email: None,
        other_password: None,
    }
}

#[tokio::test]
async fn holding_a_session_on_both_identities_is_proof() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    let (_, person_a) = who(&first);
    let (b, person_b) = who(&second);

    let committed = confirm_match(
        &harness.ctx(first.principal),
        &confirmation(&harness, candidate, Some(second.token.expose())),
    )
    .await
    .expect("two sessions are two signings-in");

    assert_eq!(
        committed.event,
        Event::PersonMerged {
            survivor: person_a,
            absorbed: person_b,
            method: LinkMethod::SelfLink,
            candidate: Some(candidate),
        }
    );
    assert_eq!(person(&harness, b).await, person_a);
}

/// A confirmation that proves with the other identity's credentials.
fn credentials(
    harness: &Local,
    candidate: Id<MatchCandidate>,
    email: &str,
    password: &str,
) -> ConfirmMatch {
    ConfirmMatch {
        candidate: public(harness, candidate),
        other_session: None,
        other_email: Some(email.to_owned()),
        other_password: Some(password.to_owned()),
    }
}

#[tokio::test]
async fn the_other_identitys_credentials_are_proof_and_mint_nothing() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    let (_, person_a) = who(&first);
    let (b, person_b) = who(&second);
    let sessions_before = count(&harness, "SELECT count(*) AS n FROM session", bind![]).await;

    // A wrong password, an address nobody holds, and an address that is not
    // the other half of this pair are the same refusal.
    for (email, password) in [
        ("two@example.test", "the wrong password entirely"),
        ("nobody@example.test", TEST_PASSWORD),
        ("one@example.test", TEST_PASSWORD),
    ] {
        let refused = confirm_match(
            &harness.ctx(first.principal.clone()),
            &credentials(&harness, candidate, email, password),
        )
        .await;
        assert!(matches!(refused, Err(ref e) if e.is_decline()), "{email}");
    }

    let committed = confirm_match(
        &harness.ctx(first.principal.clone()),
        &credentials(&harness, candidate, "two@example.test", TEST_PASSWORD),
    )
    .await
    .expect("signing in twice is signing in twice, whatever the shape");

    assert_eq!(
        committed.event,
        Event::PersonMerged {
            survivor: person_a,
            absorbed: person_b,
            method: LinkMethod::SelfLink,
            candidate: Some(candidate),
        }
    );
    assert_eq!(person(&harness, b).await, person_a);
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM session", bind![]).await,
        sessions_before,
        "proving you could sign in minted a session"
    );
}

#[tokio::test]
async fn a_factor_both_have_verified_is_proof_too() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    let (a, person_a) = who(&first);
    let (b, _) = who(&second);
    shared_subject(&harness, a, "sub-shared").await;
    shared_subject(&harness, b, "sub-shared").await;

    let committed = confirm_match(
        &harness.ctx(first.principal),
        &confirmation(&harness, candidate, None),
    )
    .await
    .expect("a proven shared factor is proof");

    assert!(matches!(
        committed.event,
        Event::PersonMerged {
            method: LinkMethod::Factor,
            ..
        }
    ));
    assert_eq!(person(&harness, b).await, person_a);
}

#[tokio::test]
async fn an_unverified_factor_is_not_proof() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    let (a, person_a) = who(&first);
    let (b, person_b) = who(&second);
    // The same value on both, proven on neither.
    for identity in [a, b] {
        write(
            harness.store(),
            "INSERT INTO factor (identity_id, kind, value, created_at) \
             VALUES ($1, 'oidc', 'claimed-not-proven', $2)",
            bind![identity, harness.store().now()],
        )
        .await;
    }

    let refused = confirm_match(
        &harness.ctx(first.principal),
        &confirmation(&harness, candidate, None),
    )
    .await;
    assert!(matches!(refused, Err(KernelError::Decline(_))));
    assert_ne!(person(&harness, b).await, person_a);
    assert_eq!(person(&harness, b).await, person_b);
}

#[tokio::test]
async fn a_signal_on_its_own_merges_nobody() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    let (_, person_a) = who(&first);
    let (b, person_b) = who(&second);

    let refused = confirm_match(
        &harness.ctx(first.principal.clone()),
        &confirmation(&harness, candidate, None),
    )
    .await;
    assert!(matches!(refused, Err(KernelError::Decline(_))));

    // The caller's own session is not a session on the *other* identity.
    let refused = confirm_match(
        &harness.ctx(first.principal),
        &confirmation(&harness, candidate, Some(first.token.expose())),
    )
    .await;
    assert!(matches!(refused, Err(KernelError::Decline(_))));

    assert_eq!(person(&harness, b).await, person_b);
    assert_ne!(person(&harness, b).await, person_a);
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM match_candidate WHERE id = $1 AND status = 'proposed'",
            bind![candidate],
        )
        .await,
        1,
        "the question stays open"
    );
}

#[tokio::test]
async fn an_expired_session_proves_nothing() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    harness.advance(crate::domain::SESSION_TTL + 1);
    let live = harness.sign_in("one@example.test").await.expect("signs in");

    let refused = confirm_match(
        &harness.ctx(live.principal),
        &confirmation(&harness, candidate, Some(second.token.expose())),
    )
    .await;
    assert!(matches!(refused, Err(KernelError::Decline(_))));
    let _ = first;
}

#[tokio::test]
async fn a_stranger_cannot_confirm_somebody_elses_pair() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    let stranger = harness
        .register("Somebody", "else@example.test")
        .await
        .expect("registers");

    let refused = confirm_match(
        &harness.ctx(stranger.principal),
        &confirmation(&harness, candidate, Some(second.token.expose())),
    )
    .await;
    assert!(matches!(refused, Err(KernelError::Decline(_))));
    let _ = first;
}

#[tokio::test]
async fn only_a_platform_operator_rules_and_only_with_evidence() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    let ruling = |evidence: &str| RuleMatch {
        candidate: public(&harness, candidate),
        same_person: true,
        evidence: evidence.to_owned(),
    };

    let refused = rule_match(&harness.ctx(first.principal.clone()), &ruling("a passport")).await;
    assert!(
        matches!(refused, Err(KernelError::Decline(_))),
        "an ordinary member does not rule on other people's identities"
    );

    operator(&harness, who(&first).1).await;
    let missing = rule_match(&harness.ctx(first.principal.clone()), &ruling("   ")).await;
    assert!(matches!(
        missing,
        Err(KernelError::Invalid(crate::error::Invalid::Missing(
            "evidence"
        )))
    ));

    let survivor = who(&first).1;
    let committed = rule_match(&harness.ctx(first.principal), &ruling("a passport"))
        .await
        .expect("an operator with evidence rules");
    assert!(matches!(committed.event, Event::PersonMerged { .. }));
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM person_link \
             WHERE person_id = $1 AND evidence = 'a passport'",
            bind![survivor],
        )
        .await,
        1,
        "the evidence is on the link row, not only in the request"
    );
    let _ = second;
}

#[tokio::test]
async fn a_ruling_can_close_a_pair_instead_of_merging_it() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    operator(&harness, who(&first).1).await;

    let committed = rule_match(
        &harness.ctx(first.principal),
        &RuleMatch {
            candidate: public(&harness, candidate),
            same_person: false,
            evidence: "two brothers, one house, one surname".into(),
        },
    )
    .await
    .expect("an operator may say no");

    assert_eq!(committed.event, Event::MatchRejected { candidate });
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM match_candidate WHERE id = $1 AND status = 'rejected'",
            bind![candidate],
        )
        .await,
        1
    );
    assert_eq!(person(&harness, who(&second).0).await, who(&second).1);
}

#[tokio::test]
async fn a_proposal_is_about_you_or_you_are_an_operator() {
    let harness = Local::new();
    let (first, second, _) = queued(&harness).await;
    let stranger = harness
        .register("Somebody", "third@example.test")
        .await
        .expect("registers");
    let args = ProposeMatch {
        identity_a: public(&harness, who(&first).0),
        identity_b: public(&harness, who(&second).0),
        signal: MatchSignal::NameAndGroup,
    };

    let refused = propose_match(&harness.ctx(stranger.principal.clone()), &args).await;
    assert!(
        matches!(refused, Err(KernelError::Decline(_))),
        "the queue is not a way to tell two strangers what a third thinks"
    );

    let committed = propose_match(&harness.ctx(first.principal), &args)
        .await
        .expect("a person may raise a pair about themselves");
    assert!(matches!(committed.event, Event::MatchProposed { .. }));

    operator(&harness, who(&stranger).1).await;
    propose_match(&harness.ctx(stranger.principal), &args)
        .await
        .expect("an operator may raise anybody's");
}

#[tokio::test]
async fn a_split_hands_back_the_rows_the_identity_arrived_with() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    let (_, person_a) = who(&first);
    let (b, person_b) = who(&second);

    let reviewers = group(&harness, "reviewers").await;
    join(&harness, reviewers, person_b, "owner").await;
    grant(&harness, 7, "editor", person_b).await;
    let before_memberships = memberships(&harness, person_b).await;
    let before_grants = grants(&harness, person_b).await;

    confirm_match(
        &harness.ctx(first.principal.clone()),
        &confirmation(&harness, candidate, Some(second.token.expose())),
    )
    .await
    .expect("merges");
    assert_eq!(person(&harness, b).await, person_a);

    let signed_in = crate::principal::resolve(harness.store(), &first.token)
        .await
        .expect("resolves");
    let committed = crate::merge::split(
        &harness.ctx(signed_in.principal),
        &Split {
            identity: public(&harness, b),
            evidence: "merged in error".into(),
        },
    )
    .await
    .expect("the person splits their own identity back out");

    let Event::PersonSplit {
        identity,
        person: fresh,
    } = committed.event
    else {
        panic!("a split produces a split event: {:?}", committed.event);
    };
    assert_eq!(identity, b);
    assert_ne!(fresh, person_a);
    assert_ne!(fresh, person_b, "a fresh person, not the absorbed one");
    assert_eq!(person(&harness, b).await, fresh);
    assert_eq!(memberships(&harness, fresh).await, before_memberships);
    assert_eq!(grants(&harness, fresh).await, before_grants);

    // The identity's own session follows it; nobody else's moves.
    let moved = crate::principal::resolve(harness.store(), &second.token)
        .await
        .expect("resolves");
    assert_eq!(moved.principal.acting_as(), Some(fresh));
}

#[tokio::test]
async fn splitting_the_only_identity_of_a_person_is_refused() {
    let harness = Local::new();
    let (first, _, _) = queued(&harness).await;
    let refused = crate::merge::split(
        &harness.ctx(first.principal.clone()),
        &Split {
            identity: public(&harness, who(&first).0),
            evidence: "no reason".into(),
        },
    )
    .await;
    assert!(
        matches!(refused, Err(KernelError::Decline(_))),
        "that is a rename with extra steps, not a split"
    );
}

#[tokio::test]
async fn a_stranger_cannot_split_somebody_elses_identity() {
    let harness = Local::new();
    let (first, second, candidate) = queued(&harness).await;
    let stranger = harness
        .register("Somebody", "fourth@example.test")
        .await
        .expect("registers");
    confirm_match(
        &harness.ctx(first.principal),
        &confirmation(&harness, candidate, Some(second.token.expose())),
    )
    .await
    .expect("merges");

    let refused = crate::merge::split(
        &harness.ctx(stranger.principal),
        &Split {
            identity: public(&harness, who(&second).0),
            evidence: "curiosity".into(),
        },
    )
    .await;
    assert!(matches!(refused, Err(KernelError::Decline(_))));
}
