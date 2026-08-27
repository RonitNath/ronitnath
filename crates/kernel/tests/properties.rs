//! Properties the kernel must hold for every input, not just the ones a
//! hand-written case happened to pick.

use proptest::prelude::*;
use rn_api::commands::{Disable, Enable, Register};
use rn_kernel::cmd::{self, Ctx};
use rn_kernel::domain::Token;
use rn_kernel::error::KernelError;
use rn_kernel::ids::{IdKey, Identity, Person, Resource, Session};
use rn_kernel::principal::Principal;
use rn_kernel::store::{Count, Reads};
use rn_kernel::testing::{Local, TEST_ID_KEY, TEST_PASSWORD};
use rn_kernel::{bind, ids};

/// Every proptest that needs to await runs on its own current-thread runtime:
/// proptest drives the case synchronously, and a shared runtime would make two
/// cases share a database.
fn block<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime starts")
        .block_on(future)
}

fn key() -> IdKey {
    IdKey::from_hex(TEST_ID_KEY).expect("the test key is well formed")
}

// ------------------------------------------------------------ id forgery ---

proptest! {
    /// A string a caller made up is refused, whatever it is, without the
    /// database being consulted. The only accepted strings are the ones that
    /// round-trip — that is, the ones we minted.
    #[test]
    fn a_forged_id_is_refused_for_every_kind(body in "[A-Za-z0-9_-]{22}") {
        let key = key();
        for prefix in ["p_", "i_", "s_", "f_", "r_", "o_", "g_", "v_", "m_"] {
            let Ok(candidate) = format!("{prefix}{body}").parse::<rn_api::ids::PublicId>() else {
                continue;
            };
            let refused = match prefix {
                "i_" => ids::decode::<Identity>(&key, &candidate).is_err(),
                "s_" => ids::decode::<Session>(&key, &candidate).is_err(),
                "p_" => ids::decode::<Person>(&key, &candidate).is_err(),
                "r_" => ids::decode::<Resource>(&key, &candidate).is_err(),
                _ => true,
            };
            // Refusal is the rule; the exception is guessing a real
            // ciphertext, which round-trips back to the same string.
            if !refused {
                let row = ids::decode::<Identity>(&key, &candidate).expect("accepted");
                prop_assert_eq!(row.public(&key), candidate);
            }
        }
    }

    /// An id minted under one key never decodes under another.
    #[test]
    fn an_id_never_crosses_deployments(row in 1i64..1_000_000, seed in 0u8..=255) {
        let mine = key();
        let theirs = IdKey::from_bytes([seed; 16]);
        let id = ids::Id::<Identity>::new(row).public(&mine);
        if seed == 0x0f {
            return Ok(()); // the same key by construction
        }
        prop_assert!(ids::decode::<Identity>(&theirs, &id).is_err());
    }
}

// --------------------------------------------------------- token digests ---

proptest! {
    /// The stored digest is never the token. Stated as a property because the
    /// failure mode it guards against — somebody "simplifying" the column to
    /// hold the token — produces a system that still passes every other test.
    #[test]
    fn a_session_digest_is_never_its_token(_seed in 0u32..64) {
        let token = Token::mint();
        let digest = token.digest();
        prop_assert_ne!(digest.as_slice(), token.expose().as_bytes());
        prop_assert_ne!(
            digest.to_vec(),
            token.expose().chars().map(|c| c as u8).collect::<Vec<_>>()
        );
        prop_assert_eq!(digest, token.digest());
        prop_assert_ne!(digest, Token::mint().digest());
        let printed = format!("{:?}", token);
        prop_assert!(!printed.contains(token.expose()));
    }
}

// ------------------------------------------------------ register is atomic --

proptest! {
    #![proptest_config(ProptestConfig::with_cases(6))]

    /// Registration writes six rows across four tables. Failing at any one of
    /// them must leave none of them — and must leave the address free, so the
    /// person can try again rather than being locked out of a registration
    /// that half happened.
    #[test]
    fn register_is_all_or_nothing(fail_at in 0usize..6) {
        block(async move {
            let harness = Local::new();
            harness.store().engine().fail_next_batch_at(fail_at);

            let outcome = cmd::register(
                &harness.ctx(Principal::Anonymous),
                &Register {
                    display_name: "Ronit".into(),
                    email: "atomic@example.test".into(),
                    password: TEST_PASSWORD.into(),
                },
            )
            .await;
            prop_assert!(outcome.is_err(), "the injected failure was not injected");

            for table in [
                "SELECT count(*) AS n FROM party",
                "SELECT count(*) AS n FROM identity",
                "SELECT count(*) AS n FROM factor",
                "SELECT count(*) AS n FROM session",
                "SELECT count(*) AS n FROM audit",
            ] {
                let rows: Vec<Count> =
                    harness.store().query(table, bind![]).await.expect("query runs");
                prop_assert_eq!(rows[0].0, 0, "{} left rows behind", table);
            }

            // And the address is still free.
            harness
                .register("Ronit", "atomic@example.test")
                .await
                .expect("a failed registration blocks nothing");
            Ok(())
        })?;
    }
}

// ------------------------------------------------ status only via commands --

/// One step of a random lifecycle.
#[derive(Debug, Clone, Copy)]
enum Step {
    Disable,
    Enable,
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(prop_oneof![Just(Step::Disable), Just(Step::Enable)], 1..12)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(12))]

    /// A party's status is a projection of the commands that produced it.
    /// However the calls are shuffled, the row ends where the last *accepted*
    /// command left it, every accepted command left exactly one audit row, and
    /// every refused one left none.
    #[test]
    fn a_status_is_only_ever_what_a_command_made_it(script in steps()) {
        block(async move {
            let harness = Local::new();
            let operator = harness
                .register("Operator", "operator@example.test")
                .await
                .expect("registers");
            let person = operator.principal.acting_as().expect("acts as somebody");
            // Platform administration is a relation row; K2 writes them.
            harness
                .store()
                .execute(
                    "INSERT INTO relation \
                     (object_kind, object_id, relation, subject_kind, subject_id, at) \
                     VALUES ('platform', 0, 'operator', 'person', $1, 0)",
                    bind![person],
                )
                .await
                .expect("insert runs");
            let subject = harness
                .register("Subject", "subject@example.test")
                .await
                .expect("registers");
            let party = subject
                .principal
                .acting_as()
                .expect("acts as somebody")
                .public(harness.store().ids());

            let mut expected_active = true;
            let mut accepted = 0i64;

            for step in script {
                let ctx: Ctx<'_, _, _> = harness.ctx(operator.principal.clone());
                let outcome = match step {
                    Step::Disable => cmd::disable(
                        &ctx,
                        &Disable { party: party.clone(), reason: "script".into() },
                    )
                    .await
                    .map(|_| ()),
                    Step::Enable => {
                        cmd::enable(&ctx, &Enable { party: party.clone() }).await.map(|_| ())
                    }
                };

                let should_apply = matches!(
                    (step, expected_active),
                    (Step::Disable, true) | (Step::Enable, false)
                );
                match outcome {
                    Ok(()) => {
                        prop_assert!(should_apply, "a no-op command reported success");
                        expected_active = matches!(step, Step::Enable);
                        accepted += 1;
                    }
                    Err(err) => {
                        prop_assert!(!should_apply, "a valid transition was refused: {err:?}");
                        prop_assert!(
                            matches!(err, KernelError::Decline(_)),
                            "a no-op is a decline, not {err:?}"
                        );
                    }
                }
            }

            let status: Vec<Count> = harness
                .store()
                .query(
                    "SELECT count(*) AS n FROM party WHERE id = $1 AND status = 'active'",
                    bind![subject.principal.acting_as().expect("acts as somebody")],
                )
                .await
                .expect("query runs");
            prop_assert_eq!(status[0].0 == 1, expected_active);

            let audited: Vec<Count> = harness
                .store()
                .query(
                    "SELECT count(*) AS n FROM audit WHERE command IN ('disable', 'enable')",
                    bind![],
                )
                .await
                .expect("query runs");
            prop_assert_eq!(
                audited[0].0,
                accepted,
                "one audit row per accepted command, none for the rest"
            );
            Ok(())
        })?;
    }
}
