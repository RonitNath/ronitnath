//! The five steps of a merge, one at a time and together.

use super::prelude::*;

/// Two people who share a verified OIDC subject, with memberships and grants
/// on both sides and a queued candidate to rule on.
struct Pair {
    operator: Registered,
    first: Registered,
    second: Registered,
    candidate: Id<MatchCandidate>,
    group: Id<Group>,
}

async fn pair(harness: &Local) -> Pair {
    let operator_who = harness
        .register("Operator", "op@example.test")
        .await
        .expect("registers");
    operator(harness, who(&operator_who).1).await;

    let first = harness
        .register("Ronit", "first@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit", "second@example.test")
        .await
        .expect("registers");
    let (a, person_a) = who(&first);
    let (b, person_b) = who(&second);

    let reviewers = group(harness, "reviewers").await;
    join(harness, reviewers, person_a, "member").await;
    join(harness, reviewers, person_b, "owner").await;
    let readers = group(harness, "readers").await;
    join(harness, readers, person_b, "member").await;

    grant(harness, 1, "viewer", person_a).await;
    grant(harness, 1, "editor", person_b).await;
    grant(harness, 2, "viewer", person_b).await;

    shared_subject(harness, a, "sub-1").await;
    shared_subject(harness, b, "sub-1").await;
    let candidate = queue(harness, a, b, MatchSignal::OidcSubject).await;

    Pair {
        operator: operator_who,
        first,
        second,
        candidate,
        group: reviewers,
    }
}

fn ruling(candidate: Id<MatchCandidate>, harness: &Local) -> RuleMatch {
    RuleMatch {
        candidate: public(harness, candidate),
        same_person: true,
        evidence: "passport shown on a support call, 2026-08-26".into(),
    }
}

#[tokio::test]
async fn a_merge_runs_all_five_steps_in_one_transaction() {
    let harness = Local::new();
    let set = pair(&harness).await;
    let (a, person_a) = who(&set.first);
    let (b, person_b) = who(&set.second);

    let committed = rule_match(
        &harness.ctx(set.operator.principal.clone()),
        &ruling(set.candidate, &harness),
    )
    .await
    .expect("the operator rules");

    // The older person survives: `first` registered before `second`.
    assert!(person_a < person_b);
    assert_eq!(
        committed.event,
        Event::PersonMerged {
            survivor: person_a,
            absorbed: person_b,
            method: LinkMethod::Operator,
            candidate: Some(set.candidate),
        }
    );

    // 1 — a person_link row per identity of the absorbed person, carrying the
    // evidence and where it came from.
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM person_link \
             WHERE identity_id = $1 AND person_id = $2 AND method = 'operator' \
               AND from_person_id = $3 AND evidence IS NOT NULL",
            bind![b, person_a, person_b],
        )
        .await,
        1
    );

    // 2 — memberships unioned, strongest role winning.
    assert_eq!(
        memberships(&harness, person_a).await,
        vec![
            format!("{}:owner", set.group.get()),
            format!("{}:member", set.group.get() + 1),
        ]
    );

    // 3 — grants unioned. Nesting is applied in code, so holding `viewer` and
    // `editor` on the same document is holding the stronger of the two.
    assert_eq!(
        grants(&harness, person_a).await,
        vec!["1:editor", "1:viewer", "2:viewer"]
    );

    // 4 — the alias, the status, and the identities themselves.
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM person_alias WHERE old_person_id = $1 AND person_id = $2",
            bind![person_b, person_a],
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM party WHERE id = $1 AND status = 'merged'",
            bind![person_b],
        )
        .await,
        1
    );
    assert_eq!(person(&harness, b).await, person_a);
    assert_eq!(person(&harness, a).await, person_a);

    // 5 — the candidate is closed by the same transaction.
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM match_candidate WHERE id = $1 AND status = 'confirmed'",
            bind![set.candidate],
        )
        .await,
        1
    );
}

#[tokio::test]
async fn a_merge_leaves_every_session_working() {
    let harness = Local::new();
    let set = pair(&harness).await;
    let (_, person_a) = who(&set.first);

    rule_match(
        &harness.ctx(set.operator.principal.clone()),
        &ruling(set.candidate, &harness),
    )
    .await
    .expect("the operator rules");

    // The absorbed person's session was never touched: same row, same token.
    let resolved = crate::principal::resolve(harness.store(), &set.second.token)
        .await
        .expect("resolves");
    assert_eq!(
        resolved.principal.session(),
        set.second.principal.session(),
        "a merge is not a sign-out"
    );
    assert_eq!(
        resolved.principal.acting_as(),
        Some(person_a),
        "but it no longer speaks as a party that has been merged away"
    );
}

#[tokio::test]
async fn merging_twice_is_merging_once() {
    let harness = Local::new();
    let set = pair(&harness).await;
    let (a, person_a) = who(&set.first);
    let (b, _) = who(&set.second);

    rule_match(
        &harness.ctx(set.operator.principal.clone()),
        &ruling(set.candidate, &harness),
    )
    .await
    .expect("the operator rules");
    let links = count(&harness, "SELECT count(*) AS n FROM person_link", bind![]).await;
    let aliases = count(&harness, "SELECT count(*) AS n FROM person_alias", bind![]).await;
    let held = grants(&harness, person_a).await;

    // A second candidate for the same pair, a second ruling: the two are
    // already one person, so there is nothing to write.
    let again = queue(&harness, a, b, MatchSignal::NameAndGroup).await;
    let refused = rule_match(
        &harness.ctx(set.operator.principal),
        &ruling(again, &harness),
    )
    .await;
    assert!(matches!(refused, Err(KernelError::Decline(_))));

    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM person_link", bind![]).await,
        links
    );
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM person_alias", bind![]).await,
        aliases
    );
    assert_eq!(grants(&harness, person_a).await, held);
}

#[tokio::test]
async fn an_identity_with_no_person_is_linked_rather_than_merged() {
    let harness = Local::new();
    let set = pair(&harness).await;
    let (a, person_a) = who(&set.first);
    let (b, person_b) = who(&set.second);

    // An identity that arrived from an import and has not been resolved yet.
    write(
        harness.store(),
        "UPDATE identity SET person_id = NULL WHERE id = $1",
        bind![b],
    )
    .await;

    let committed = rule_match(
        &harness.ctx(set.operator.principal),
        &ruling(set.candidate, &harness),
    )
    .await
    .expect("the operator rules");

    assert_eq!(
        committed.event,
        Event::IdentityLinked {
            identity: b,
            person: person_a,
            method: LinkMethod::Operator,
        }
    );
    assert_eq!(person(&harness, b).await, person_a);
    assert_eq!(person(&harness, a).await, person_a);
    // Nothing was absorbed, so nothing is an alias and no status moved.
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM person_alias", bind![]).await,
        0
    );
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM party WHERE id = $1 AND status = 'active'",
            bind![person_b],
        )
        .await,
        1
    );
}

#[tokio::test]
async fn an_absorbed_persons_id_keeps_resolving() {
    let harness = Local::new();
    let set = pair(&harness).await;
    let (_, person_a) = who(&set.first);
    let (_, person_b) = who(&set.second);

    rule_match(
        &harness.ctx(set.operator.principal),
        &ruling(set.candidate, &harness),
    )
    .await
    .expect("the operator rules");

    assert_eq!(
        resolve_person(&harness.store().reads(), person_b)
            .await
            .expect("resolves"),
        person_a,
        "an old URL still names a person"
    );
    assert_eq!(
        resolve_person(&harness.store().reads(), person_a)
            .await
            .expect("resolves"),
        person_a
    );
}

#[tokio::test]
async fn a_chain_of_merges_stays_one_hop_long() {
    let harness = Local::new();
    let set = pair(&harness).await;
    let (a, person_a) = who(&set.first);
    let (_, person_b) = who(&set.second);

    rule_match(
        &harness.ctx(set.operator.principal.clone()),
        &ruling(set.candidate, &harness),
    )
    .await
    .expect("the operator rules");

    let third = harness
        .register("Ronit", "third@example.test")
        .await
        .expect("registers");
    let (c, person_c) = who(&third);
    shared_subject(&harness, c, "sub-2").await;
    shared_subject(&harness, a, "sub-2").await;
    let candidate = queue(&harness, a, c, MatchSignal::OidcSubject).await;
    rule_match(
        &harness.ctx(set.operator.principal),
        &ruling(candidate, &harness),
    )
    .await
    .expect("the operator rules again");

    assert_eq!(
        resolve_person(&harness.store().reads(), person_b)
            .await
            .expect("resolves"),
        person_a
    );
    assert_eq!(
        resolve_person(&harness.store().reads(), person_c)
            .await
            .expect("resolves"),
        person_a
    );
    // Repointed rather than chained: every alias names the survivor directly.
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM person_alias WHERE person_id = $1",
            bind![person_a],
        )
        .await,
        2
    );
}

#[tokio::test]
async fn person_link_refuses_to_be_edited_or_deleted() {
    let harness = Local::new();
    let set = pair(&harness).await;
    rule_match(
        &harness.ctx(set.operator.principal),
        &ruling(set.candidate, &harness),
    )
    .await
    .expect("the operator rules");

    for sql in [
        "UPDATE person_link SET evidence = 'something else'",
        "DELETE FROM person_link",
    ] {
        let refused = harness.store().execute(sql, bind![]).await;
        assert!(
            refused.is_err(),
            "person_link is append-only by trigger, not by convention: {sql}"
        );
    }
}

#[tokio::test]
async fn a_merged_person_rotates_the_lost_registrations_factors() {
    let harness = Local::new();
    let set = pair(&harness).await;
    let (a, _) = who(&set.first);
    let (b, _) = who(&set.second);

    // Before the merge, one registration is nobody else's business.
    let refused = crate::cmd::add_factor(
        &harness.ctx(set.first.principal.clone()),
        &rn_api::commands::AddFactor {
            kind: rn_api::commands::FactorKind::Email,
            value: "rotated@example.test".into(),
            identity: Some(public(&harness, b)),
        },
    )
    .await;
    assert!(matches!(refused, Err(KernelError::Decline(_))));

    rule_match(
        &harness.ctx(set.operator.principal),
        &ruling(set.candidate, &harness),
    )
    .await
    .expect("the operator rules");

    // After it, the person signs in with the registration they still hold and
    // rotates the factors of the one they lost. This is the whole reason the
    // operator path exists.
    let signed_in = crate::principal::resolve(harness.store(), &set.first.token)
        .await
        .expect("resolves");
    let committed = crate::cmd::add_factor(
        &harness.ctx(signed_in.principal),
        &rn_api::commands::AddFactor {
            kind: rn_api::commands::FactorKind::Email,
            value: "rotated@example.test".into(),
            identity: Some(public(&harness, b)),
        },
    )
    .await
    .expect("the person may act on their own identity");
    assert_eq!(committed.event.touches_identity(), Some(b));
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM factor \
             WHERE identity_id = $1 AND value = 'rotated@example.test'",
            bind![b],
        )
        .await,
        1
    );
    // And the audit row still says who actually did it.
    assert_eq!(
        count(
            &harness,
            "SELECT count(*) AS n FROM audit \
             WHERE command = 'add-factor' AND actor_identity_id = $1",
            bind![a],
        )
        .await,
        1
    );
}

/// The `IN (…)` list a column's CHECK constraint carries, out of the schema
/// itself — the same check `domain::tests` runs for K1's vocabularies, over a
/// flattened DDL because `match_candidate.signal`'s list wraps a line.
async fn admitted(harness: &Local, table: &'static str, column: &str) -> Vec<String> {
    #[derive(Debug)]
    struct Ddl(String);
    impl crate::store::FromRow for Ddl {
        fn from_row(row: &mut impl crate::store::Cursor) -> Result<Self, crate::store::RowError> {
            Ok(Self(row.text("sql")?))
        }
    }
    let ddl = harness
        .store()
        .query_one::<Ddl>(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = $1",
            bind![table],
        )
        .await
        .expect("the table exists")
        .0;
    let flat = ddl.split_whitespace().collect::<Vec<_>>().join(" ");
    let at = flat
        .find(&format!("{column} TEXT"))
        .unwrap_or_else(|| panic!("no {column} column on {table}"));
    let rest = &flat[at..];
    let open = rest.find("IN (").expect("a CHECK with an IN list") + 4;
    let close = rest[open..].find(')').expect("a closed IN list") + open;
    rest[open..close]
        .split(',')
        .map(|v| v.trim().trim_matches('\'').to_owned())
        .collect()
}

async fn agrees<V: Vocabulary>(harness: &Local) {
    let (table, column) = V::COLUMN;
    let code: Vec<String> = V::ALL.iter().map(|v| v.as_str().to_owned()).collect();
    assert_eq!(
        admitted(harness, table, column).await,
        code,
        "{table}.{column}: the schema and the enum disagree"
    );
    assert!(V::parse("something else").is_none());
}

#[tokio::test]
async fn the_merge_vocabularies_agree_with_the_schema() {
    let harness = Local::new();
    agrees::<LinkMethod>(&harness).await;
    agrees::<CandidateStatus>(&harness).await;
    agrees::<MatchSignal>(&harness).await;
}
