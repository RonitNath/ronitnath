//! The platform operator's own commands: delegation, the shorter session, the
//! re-authentication window, and impersonation.
//!
//! The sweep in [`super::operator`] proves that an operator *reaches* every
//! command. These are about the four things that are true only of an operator,
//! and each one is a rule that would be invisible in a functional test: a
//! session that is too long, a password that was presented too long ago, a hat
//! that outlived the head wearing it.

use rn_api::commands::{
    AddFactor, CreateDocument, Disable, EndImpersonation, FactorKind, GrantOperator,
    ReAuthenticate, RevokeOperator, SignInAs,
};

use super::prelude::*;
use super::world::{self, key, public};
use crate::authority::{self, PLATFORM_REAUTH_WINDOW};
use crate::cmd;
use crate::domain::{IMPERSONATION_TTL, OPERATOR_SESSION_TTL};
use crate::ids::{Id, Person};
use crate::store::Value;

/// A ctx whose deployment forbids impersonation, for requirement C11.3.
fn without_impersonation<'a>(
    harness: &'a Local,
    principal: Principal,
) -> crate::cmd::Ctx<'a, crate::store::Sqlite, crate::feed::LocalFeed<crate::store::Sqlite>> {
    let mut ctx = harness.ctx(principal);
    ctx.impersonation = false;
    ctx
}

// ------------------------------------------------------------ A2 delegation -

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

// --------------------------------------------------- A3.1 the shorter life --

#[tokio::test]
async fn an_operators_session_is_a_working_day_and_everybody_elses_is_a_fortnight() {
    let harness = Local::new();
    let operator = world::person(&harness, "Op", "ttl-op@example.test").await;
    world::person(&harness, "Member", "ttl-member@example.test").await;
    make_operator(&harness, operator.person()).await;

    // Same clock, two sign-ins.
    let their_session = harness
        .sign_in("ttl-op@example.test")
        .await
        .expect("signs in")
        .principal
        .session()
        .expect("a member holds one");
    let ordinary_session = harness
        .sign_in("ttl-member@example.test")
        .await
        .expect("signs in")
        .principal
        .session()
        .expect("a member holds one");

    let theirs = expires_at(&harness, their_session).await;
    let ordinary = expires_at(&harness, ordinary_session).await;
    assert_eq!(
        ordinary - theirs,
        SESSION_TTL - OPERATOR_SESSION_TTL,
        "the difference is exactly one TTL against the other, not a rounding"
    );
}

#[tokio::test]
async fn a_promotion_shortens_the_session_it_promotes() {
    let harness = Local::new();
    let a = world::person(&harness, "A", "short-a@example.test").await;
    let b = world::person(&harness, "B", "short-b@example.test").await;
    make_operator(&harness, a.person()).await;

    let session = b.principal.session().expect("a member holds one");
    assert_eq!(
        expires_at(&harness, session).await,
        crate::testing::TEST_EPOCH + SESSION_TTL,
        "registered before the grant, so a fortnight"
    );

    cmd::grant_operator(
        &harness.ctx(a.principal.clone()),
        &GrantOperator {
            person: public(key(&harness), b.person()),
            reason: "covering the weekend".into(),
        },
    )
    .await
    .expect("delegates");

    assert_eq!(
        expires_at(&harness, session).await,
        crate::testing::TEST_EPOCH + OPERATOR_SESSION_TTL,
        "a grant that left a fortnight in place would be a fortnight of god mode"
    );
}

// ----------------------------------------------- A3.2 the re-auth window ---

#[tokio::test]
async fn a_sensitive_command_asks_for_the_password_again_and_says_so() {
    let harness = Local::new();
    let a = world::person(&harness, "A", "reauth-a@example.test").await;
    let b = world::person(&harness, "B", "reauth-b@example.test").await;
    make_operator(&harness, a.person()).await;
    let grant = GrantOperator {
        person: public(key(&harness), b.person()),
        reason: "covering the weekend".into(),
    };

    // Sixteen minutes after the password was presented.
    harness.advance(PLATFORM_REAUTH_WINDOW + 60);
    let refused = cmd::grant_operator(&harness.ctx(a.principal.clone()), &grant).await;
    assert!(
        refused.as_ref().is_err_and(KernelError::is_reauth_required),
        "the one refusal a caller inside the tier is told apart, got {refused:?}"
    );
    assert!(
        !refused.as_ref().unwrap_err().is_decline(),
        "and it is not the uniform decline"
    );

    // A command that is not sensitive is unaffected — the window is a rule
    // about consequences, not a second session lifetime.
    cmd::create_document(
        &harness.ctx(a.principal.clone()),
        &CreateDocument {
            title: "Still working".into(),
            body: "The window is about consequences".into(),
            owner: None,
        },
    )
    .await
    .expect("an ordinary command does not ask");

    // Present it again, and the same command goes through. No new session:
    // the row is the one it was, with a later `auth_time`.
    let before = crate::testing::TEST_EPOCH + PLATFORM_REAUTH_WINDOW + 60;
    let sessions = count(&harness, "SELECT count(*) AS n FROM session").await;
    let committed = cmd::reauthenticate(
        &harness.ctx(a.principal.clone()),
        &ReAuthenticate {
            password: crate::testing::TEST_PASSWORD.to_owned(),
        },
    )
    .await
    .expect("the password is right");
    assert!(matches!(committed.event, Event::ReAuthenticated { .. }));
    assert_eq!(
        count(&harness, "SELECT count(*) AS n FROM session").await,
        sessions,
        "re-authenticating mints nothing"
    );
    assert_eq!(
        auth_time(&harness, a.principal.session().expect("a member")).await,
        before
    );

    cmd::grant_operator(&harness.ctx(a.principal.clone()), &grant)
        .await
        .expect("the window is open again");

    // A wrong password is the uniform decline and moves nothing.
    let refused = cmd::reauthenticate(
        &harness.ctx(a.principal.clone()),
        &ReAuthenticate {
            password: "not the password".into(),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));
}

#[tokio::test]
async fn the_password_a_re_authentication_carries_is_not_in_the_audit_row() {
    let harness = Local::new();
    let a = world::person(&harness, "A", "digest-a@example.test").await;
    cmd::reauthenticate(
        &harness.ctx(a.principal.clone()),
        &ReAuthenticate {
            password: crate::testing::TEST_PASSWORD.to_owned(),
        },
    )
    .await
    .expect("the password is right");

    // `run` digests whatever it is handed into `request_digest`. The one thing
    // this command carries is a password, so what it is handed is the identity
    // and the session instead — a digest of a password would be an offline
    // guessing target sitting in a table that is also the change feed.
    let digest = crate::audit::digest_of(&serde_json::json!({
        "identity": a.principal.identity().expect("a member").get(),
        "session": a.principal.session().expect("a member").get(),
    }));
    let matched = count_of(
        &harness,
        "SELECT count(*) AS n FROM audit WHERE command = 're-authenticate' \
           AND request_digest = $1",
        bind![digest.as_str()],
    )
    .await;
    assert_eq!(matched, 1, "the digest is over the redacted arguments");
}

// ------------------------------------------------------ C11.2 impersonation -

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
        &super::world::Who {
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

// ------------------------------------------------------------------ helpers -

async fn expires_at(harness: &Local, session: crate::ids::Id<crate::ids::Session>) -> i64 {
    count_of(
        harness,
        "SELECT expires_at AS n FROM session WHERE id = $1",
        bind![session],
    )
    .await
}

async fn auth_time(harness: &Local, session: crate::ids::Id<crate::ids::Session>) -> i64 {
    count_of(
        harness,
        "SELECT auth_time AS n FROM session WHERE id = $1",
        bind![session],
    )
    .await
}

async fn count_of(harness: &Local, sql: &'static str, params: Vec<Value>) -> i64 {
    harness
        .store()
        .query::<Count>(sql, params)
        .await
        .expect("the query runs")[0]
        .0
}

const _: Option<Id<Person>> = None;
