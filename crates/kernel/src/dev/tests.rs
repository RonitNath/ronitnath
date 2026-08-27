//! What the developer sign-in is allowed to be: a real session, a row that
//! says it was not earned, and the same refusals a password sign-in gives.

use super::*;
use crate::domain::{PartyStatus, Vocabulary};
use crate::event::Event;
use crate::principal::Principal;
use crate::store::Count;
use crate::testing::Local;

/// The `command` and the payload's `event` of every audit row, in order.
async fn tail(harness: &Local) -> Vec<(String, String)> {
    struct Row(String, String);
    impl crate::store::FromRow for Row {
        fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
            Ok(Self(row.text("command")?, row.text("payload")?))
        }
    }
    harness
        .store()
        .query::<Row>(
            "SELECT command, payload FROM audit ORDER BY id",
            crate::bind![],
        )
        .await
        .expect("the audit tail")
        .into_iter()
        .map(|row| {
            let payload: serde_json::Value = serde_json::from_str(&row.1).expect("a payload");
            (
                row.0,
                payload["event"].as_str().expect("an event tag").to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn the_session_is_real_and_the_row_says_it_was_not_earned() {
    let harness = Local::new();
    let somebody = harness
        .register("Dev Operator", "dev-kernel@example.invalid")
        .await
        .expect("a registration");
    let Principal::Member { identity, .. } = somebody.principal else {
        panic!("a registration produces a member");
    };

    let minted = sign_in_as(&harness.ctx(Principal::Anonymous), identity)
        .await
        .expect("a developer sign-in");
    let token = minted.token.expect("a fresh mint returns its token");

    // The session resolves the way any other session does — same table, same
    // resolution, same person.
    let resolved = crate::principal::resolve(harness.store(), &token)
        .await
        .expect("the session resolves");
    let Principal::Member {
        identity: signed_in,
        ..
    } = resolved.principal
    else {
        panic!("the developer session is a member");
    };
    assert_eq!(signed_in, identity);

    // The audit row carries its own name, and the payload is a sign-in — so
    // the record says what happened and the feed says what exists.
    assert_eq!(
        tail(&harness).await,
        vec![
            ("register".to_owned(), "register".to_owned()),
            (ACTION.to_owned(), "sign-in".to_owned()),
        ]
    );
    assert_eq!(ACTION, "dev-sign-in");
    assert!(
        matches!(minted.committed.event, Event::SignedIn { .. }),
        "the feed carries a sign-in, so the invalidator and the subscriptions see it"
    );
}

#[tokio::test]
async fn a_disabled_identity_or_person_is_refused_exactly_as_a_password_is() {
    let harness = Local::new();
    let somebody = harness
        .register("Shut Out", "dev-disabled@example.invalid")
        .await
        .expect("a registration");
    let Principal::Member {
        identity, person, ..
    } = somebody.principal
    else {
        panic!("a registration produces a member");
    };
    let person = person.expect("a registration resolves its person");

    harness
        .store()
        .execute(
            "UPDATE party SET status = $1 WHERE id = $2",
            crate::bind![PartyStatus::Disabled.as_str(), person],
        )
        .await
        .expect("the disable");
    assert!(
        sign_in_as(&harness.ctx(Principal::Anonymous), identity)
            .await
            .is_err(),
        "a disabled person cannot be signed in as"
    );

    harness
        .store()
        .execute(
            "UPDATE party SET status = $1 WHERE id = $2",
            crate::bind![PartyStatus::Active.as_str(), person],
        )
        .await
        .expect("the enable");
    // `identity.status = 'disabled'` is admitted by the schema and produced by
    // no command (`IdentityStatus::ADMITTED_UNPRODUCED`), so it is written
    // here as the row rather than through one — the guard has to hold against
    // a value that only the database can tell it about.
    harness
        .store()
        .execute(
            "UPDATE identity SET status = 'disabled' WHERE id = $1",
            crate::bind![identity],
        )
        .await
        .expect("the identity disable");
    assert!(
        sign_in_as(&harness.ctx(Principal::Anonymous), identity)
            .await
            .is_err(),
        "a disabled identity cannot be signed in as"
    );

    // An identity nobody has is the same refusal, and it wrote nothing.
    assert!(
        sign_in_as(&harness.ctx(Principal::Anonymous), Id::new(9_999))
            .await
            .is_err()
    );
    let sessions = harness
        .store()
        .query::<Count>("SELECT count(*) AS n FROM session", crate::bind![])
        .await
        .expect("the session count")[0]
        .0;
    assert_eq!(sessions, 1, "only the registration's own session exists");
}

#[tokio::test]
async fn one_idempotency_key_mints_one_session_however_often_it_arrives() {
    let harness = Local::new();
    let somebody = harness
        .register("Twice", "dev-replay@example.invalid")
        .await
        .expect("a registration");
    let Principal::Member { identity, .. } = somebody.principal else {
        panic!("a registration produces a member");
    };

    let key = uuid::Uuid::new_v4();
    let first = sign_in_as(&harness.ctx_keyed(Principal::Anonymous, key), identity)
        .await
        .expect("the first call");
    let replay = sign_in_as(&harness.ctx_keyed(Principal::Anonymous, key), identity)
        .await
        .expect("the replay");

    assert!(first.token.is_some());
    assert!(
        replay.token.is_none(),
        "a replay has no secret to hand back: the row keeps only the digest"
    );
    assert_eq!(first.committed.offset, replay.committed.offset);
    assert_eq!(
        tail(&harness)
            .await
            .iter()
            .filter(|(command, _)| command == ACTION)
            .count(),
        1
    );
}
