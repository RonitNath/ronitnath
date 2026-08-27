//! A3 — the shorter operator session, and the re-authentication window.

use rn_api::commands::{CreateDocument, GrantOperator, ReAuthenticate};

use super::super::prelude::*;
use super::super::world::{self, key, public};
use super::auth_time;
use super::{count_of, expires_at};
use crate::authority::PLATFORM_REAUTH_WINDOW;
use crate::cmd;
use crate::domain::OPERATOR_SESSION_TTL;

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
