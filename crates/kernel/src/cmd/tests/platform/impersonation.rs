//! C11.2 and C11.3 — wearing somebody else's session, and the deployment's
//! right to forbid it.

use rn_api::commands::{
    AddFactor, CreateDocument, Disable, EndImpersonation, FactorKind, RevokeOperator, SignInAs,
};

use super::super::prelude::*;
use super::super::world::{self, key, public};
use super::without_impersonation;
use super::{count_of, expires_at};
use crate::cmd;
use crate::domain::IMPERSONATION_TTL;

#[tokio::test]
async fn an_operator_wears_somebody_elses_session_and_every_clause_holds() {
    let harness = Local::new();
    let operator = world::person(&harness, "Op", "imp-op@example.test").await;
    let b = world::person(&harness, "B", "imp-b@example.test").await;
    make_operator(&harness, operator.person()).await;
    let args = SignInAs {
        person: public(key(&harness), b.person()),
        reason: "reproducing the bug they reported".into(),
    };

    // C11.3 first: a deployment that says no refuses an operator who is
    // otherwise entitled, and refuses it as the uniform decline.
    let refused = cmd::sign_in_as(
        &without_impersonation(&harness, operator.principal.clone()),
        &args,
    )
    .await;
    assert!(
        matches!(refused, Err(ref e) if e.is_decline()),
        "the deployment's answer is the same shape as every other refusal"
    );

    let minted = cmd::sign_in_as(&harness.ctx(operator.principal.clone()), &args)
        .await
        .expect("an operator may");
    let token = minted.token.expect("a fresh mint returns its token");
    let Event::Impersonated { session, .. } = minted.committed.event else {
        panic!("signing in as somebody produces an impersonation");
    };

    // The session is thirty minutes, not a fortnight and not a working day.
    assert_eq!(
        expires_at(&harness, session).await,
        crate::testing::TEST_EPOCH + IMPERSONATION_TTL
    );

    let hat = crate::principal::resolve(harness.store(), &token)
        .await
        .expect("resolves")
        .principal;
    assert_eq!(
        hat.impersonated_by(),
        operator.principal.identity(),
        "the row knows who is wearing it"
    );

    // (a) The document's owner is B. The principal expands to B's real subject
    //     set, so nothing in `check()` learned a new case.
    let document = world::document(
        &harness,
        &world::Who {
            principal: hat.clone(),
        },
        "Written while wearing the hat",
        None,
    )
    .await
    .expect("creates");
    let owner = count_of(
        &harness,
        "SELECT owner_party_id AS n FROM resource WHERE id = $1",
        bind![document],
    )
    .await;
    assert_eq!(owner, b.person().get(), "the document is B's");

    // (b) The audit row names the operator as the actor and B as the hat. One
    //     function — `refs::actor` — so this is true of every command, not
    //     just this one.
    let attributed = count_of(
        &harness,
        "SELECT count(*) AS n FROM audit \
         WHERE command = 'create-document' AND actor_identity_id = $1 AND acting_as = $2",
        bind![operator.principal.identity().expect("a member"), b.person()],
    )
    .await;
    assert_eq!(attributed, 1);

    // (c) B can find what was done to them, by the object rather than by the
    //     actor — which is finding F4 and the whole reason `audit_object`
    //     exists. The query over it is a later leg's; the rows are here.
    let about_b = count_of(
        &harness,
        "SELECT count(*) AS n FROM audit_object WHERE kind = 'party' AND id = $1",
        bind![b.person()],
    )
    .await;
    assert!(about_b >= 1, "the impersonation is filed under the person");

    // (d) It may not change what B *is*. Ten commands, one rule; this is the
    //     one that would outlive the thirty minutes.
    let refused = cmd::add_factor(
        &harness.ctx(hat.clone()),
        &AddFactor {
            kind: FactorKind::Email,
            value: "smuggled@example.test".into(),
            identity: None,
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));
    let refused = cmd::sign_in_as(&harness.ctx(hat.clone()), &args).await;
    assert!(
        matches!(refused, Err(ref e) if e.is_decline()),
        "and it may not put on a second hat"
    );
    let refused = cmd::disable(
        &harness.ctx(hat.clone()),
        &Disable {
            party: public(key(&harness), b.person()),
            reason: "from inside the hat".into(),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));

    // (f) Taking it off leaves the operator's own session working. Done before
    //     (e) so the clock is still where the assertions above put it.
    cmd::end_impersonation(&harness.ctx(hat.clone()), &EndImpersonation {})
        .await
        .expect("the hat comes off");
    assert_eq!(
        crate::principal::resolve(harness.store(), &token)
            .await
            .expect("resolves")
            .principal,
        Principal::Anonymous,
        "the session it deleted resolves to nobody"
    );
    cmd::create_document(
        &harness.ctx(operator.principal.clone()),
        &CreateDocument {
            title: "Back to my own".into(),
            body: "The operator's own cookie was never touched".into(),
            owner: None,
        },
    )
    .await
    .expect("the operator's own session still works");

    // (e) And the thirty minutes are real: a second hat, unworn, expires.
    let again = cmd::sign_in_as(&harness.ctx(operator.principal.clone()), &args)
        .await
        .expect("an operator may");
    let token = again.token.expect("a fresh mint returns its token");
    harness.advance(IMPERSONATION_TTL + 1);
    assert_eq!(
        crate::principal::resolve(harness.store(), &token)
            .await
            .expect("resolves")
            .principal,
        Principal::Anonymous,
        "thirty minutes, and it is nobody"
    );
}

#[tokio::test]
async fn an_operator_may_not_become_another_operator() {
    let harness = Local::new();
    let a = world::person(&harness, "A", "two-a@example.test").await;
    let b = world::person(&harness, "B", "two-b@example.test").await;
    make_operator(&harness, a.person()).await;
    make_operator(&harness, b.person()).await;

    let refused = cmd::sign_in_as(
        &harness.ctx(a.principal.clone()),
        &SignInAs {
            person: public(key(&harness), b.person()),
            reason: "because I can".into(),
        },
    )
    .await;
    assert!(
        matches!(refused, Err(ref e) if e.is_decline()),
        "an operator who can become another operator makes revocation meaningless"
    );
}

#[tokio::test]
async fn losing_the_relation_takes_the_hat_off_at_the_next_request() {
    let harness = Local::new();
    let a = world::person(&harness, "A", "lose-a@example.test").await;
    let b = world::person(&harness, "B", "lose-b@example.test").await;
    let c = world::person(&harness, "C", "lose-c@example.test").await;
    make_operator(&harness, a.person()).await;
    make_operator(&harness, c.person()).await;

    let minted = cmd::sign_in_as(
        &harness.ctx(a.principal.clone()),
        &SignInAs {
            person: public(key(&harness), b.person()),
            reason: "looking at what they see".into(),
        },
    )
    .await
    .expect("an operator may");
    let token = minted.token.expect("a fresh mint returns its token");
    assert!(matches!(
        crate::principal::resolve(harness.store(), &token)
            .await
            .expect("resolves")
            .principal,
        Principal::Member { .. }
    ));

    // C revokes A. Nothing knows which hats A was wearing — which is exactly
    // why the check is at resolve time and not on the event.
    cmd::revoke_operator(
        &harness.ctx(c.principal.clone()),
        &RevokeOperator {
            person: public(key(&harness), a.person()),
            reason: "A is no longer with us".into(),
        },
    )
    .await
    .expect("revokes");

    assert_eq!(
        crate::principal::resolve(harness.store(), &token)
            .await
            .expect("resolves")
            .principal,
        Principal::Anonymous,
        "the hat outliving the head is the one ending that has no event"
    );
    // And A's own session is untouched: losing the relation is not a sign-out.
    assert!(matches!(
        crate::principal::resolve_digest(harness.store(), &crate::principal::Digest::default())
            .await
            .expect("resolves")
            .principal,
        Principal::Anonymous
    ));
}
