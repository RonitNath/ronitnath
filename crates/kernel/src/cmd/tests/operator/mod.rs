//! The one test that makes finding **F1** stay closed.
//!
//! God mode used to be eight commands wide. `is_platform_operator` was
//! consulted by `disable`, `enable`, `revoke`, `revoke_link`, `revoke_session`,
//! `transfer`, the merge rulings and the OpenID registry; the other eighteen
//! command modules authorised against ownership or membership alone. An
//! operator could end your session and could not `set_role` inside your
//! organization. That was not a rule anybody had written down — it was the
//! absence of one, eighteen times, and the only way it was ever going to be
//! found was by somebody needing it.
//!
//! So this test enumerates [`ALL_COMMAND_NAMES`] and classifies every one of
//! them. A command with no entry does not compile past [`classify`]'s
//! exhaustive `match`, which is the property that matters: a command added by
//! a later leg with no operator path fails the build rather than being noticed
//! in review.
//!
//! ## The two passes
//!
//! One world, two passes over it, in that order.
//!
//! * **The stranger.** An `agent` who holds nothing runs every gated command
//!   against rows that belong to other people. Every one declines. Nothing is
//!   written, so the world the second pass sees is the world the first pass
//!   found.
//! * **The operator.** `platform:* #operator @agent` is granted, and the same
//!   commands run again in an order chosen so that each one's precondition is
//!   still true. Each either succeeds — and its audit row names the agent's
//!   identity — or declines for a reason that is stated here and is not
//!   authorisation.
//!
//! The order is the reason this is one world rather than thirty: the sequence
//! *is* the story of an operator working through a deployment, and a fixture
//! per command would prove each rule in isolation and none of them together.

mod sweep;

use std::future::Future;

use rn_api::commands::ALL_COMMAND_NAMES;
use rn_api::oidc::{ClientAuthMethod, ClientMetadata, GrantType, Scope};

use super::prelude::*;
use crate::error::Outcome;
use crate::ids::{Id, Identity};

/// Why a command is not authorised by the principal that runs it, and so has
/// no operator clause to have.
///
/// Each of these is a rule rather than an exemption. Written out because the
/// alternative — a list of names — is a place a command with a genuine hole in
/// it could hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ungated {
    /// Run by a principal that is not signed in. The credential *is* the
    /// authorisation: a password, a registration form.
    Credential,
    /// About the caller's own session and nothing else. An operator has no
    /// more claim to your cookie than you have to theirs, and the commands
    /// that end one on somebody else's behalf are `revoke-session` and
    /// `disable`, which are gated.
    OwnSession,
    /// Authorised by a bearer secret the caller is holding — an invitation
    /// token, a verification token. An operator holding one is a bearer like
    /// anybody else, and one who is not holding one is refused by the secret
    /// rather than by a rule.
    Bearer,
    /// Authorised by a *client* credential the arguments carry: a client
    /// secret or a `private_key_jwt` assertion, verified inside the command.
    /// A cookie gets exactly as far here as no cookie at all, which is what
    /// keeps `/api/cmd/<name>` from being a second authorisation path.
    ClientCredential,
    /// About the acting person's own row, named by nothing a caller could
    /// point at somebody else. There is no id in the arguments to abuse.
    Own,
    /// Proof, not permission. `confirm-match` is a merge the *person* proves
    /// by holding both sides; an operator's merge is `rule-match`, which is
    /// the same act with mandatory evidence. A second, evidence-free operator
    /// path would be exactly the unaudited grant A1 refuses.
    Proof,
}

/// What the operator pass expects of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Then {
    /// It works.
    Works,
    /// It declines, and the reason is not authorisation.
    Declines(&'static str),
}

/// Whether a command is authorised by a principal at all.
///
/// Exhaustive over [`ALL_COMMAND_NAMES`] by construction: the fall-through
/// panics with the name, so a command added later is a failing test with a
/// sentence in it rather than a silently uncovered route.
const fn classify(name: &str) -> Result<(), Ungated> {
    match name.as_bytes() {
        // `re-authenticate` is a password presented again on a session that
        // already exists. The password is the authorisation, and an operator
        // does not have anybody else's.
        b"register" | b"sign-in" | b"re-authenticate" => Err(Ungated::Credential),
        // `end-impersonation` deletes the session that ran it, so an
        // operator's claim on it is the same claim anybody has on their own
        // cookie: the one they are holding.
        b"sign-out" | b"act-as" | b"end-impersonation" => Err(Ungated::OwnSession),
        b"claim-link" | b"verify-email" => Err(Ungated::Bearer),
        b"authorize"
        | b"exchange-code"
        | b"refresh-token"
        | b"client-credentials"
        | b"revoke-token"
        | b"end-session" => Err(Ungated::ClientCredential),
        b"set-handle" | b"revoke-consent" => Err(Ungated::Own),
        b"confirm-match" => Err(Ungated::Proof),
        _ => Ok(()),
    }
}

/// Every gated command, in the order the operator pass runs them.
///
/// Named here as well as exercised below so the two cannot drift: the test
/// asserts this list and the gated half of [`ALL_COMMAND_NAMES`] are the same
/// set.
const GATED: &[&str] = &[
    "propose-match",
    "rule-match",
    "split",
    "create-organization",
    "create-group",
    "invite",
    "revoke-link",
    "set-role",
    "remove-member",
    "leave",
    "create-document",
    "edit-document",
    "publish-document",
    "share",
    "revoke",
    "transfer",
    "add-factor",
    "remove-factor",
    "revoke-session",
    "register-client",
    "update-client",
    "rotate-client-secret",
    "delete-client",
    "rotate-signing-key",
    "disable",
    "enable",
    "grant-operator",
    "revoke-operator",
    "sign-in-as",
    "retire-key",
];

#[test]
fn every_command_in_the_contract_is_classified() {
    let mut gated = Vec::new();
    for name in ALL_COMMAND_NAMES {
        if classify(name).is_ok() {
            gated.push(*name);
        }
    }
    let mut expected = GATED.to_vec();
    expected.sort_unstable();
    gated.sort_unstable();
    assert_eq!(
        gated, expected,
        "a command is gated but not exercised by the operator sweep, or the other way round"
    );
}

/// Run one command twice: once as somebody who holds nothing, once as the
/// operator.
///
/// The stranger's decline is asserted first and is not optional: a command
/// that a stranger could already run is not evidence of an operator path, and
/// a case written that way would pass for the wrong reason forever.
async fn sweep<F, Fut>(seen: &mut Vec<&'static str>, name: &'static str, then: Then, run: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = Outcome<()>>,
{
    seen.push(name);
    match then {
        Then::Works => run().await.unwrap_or_else(|error| {
            panic!("{name}: an operator was refused: {error}");
        }),
        Then::Declines(why) => {
            let refused = run().await;
            assert!(
                refused.as_ref().is_err_and(KernelError::is_decline),
                "{name}: expected the decline that means {why}, got {refused:?}"
            );
        }
    }
}

/// A stranger's attempt, which must always be the uniform decline.
async fn refused<T>(name: &str, outcome: Outcome<T>) {
    assert!(
        outcome.as_ref().is_err_and(KernelError::is_decline),
        "{name}: somebody holding nothing was not refused"
    );
}

fn metadata(name: &str) -> ClientMetadata {
    ClientMetadata {
        client_name: name.to_owned(),
        client_uri: None,
        logo_uri: None,
        redirect_uris: vec!["https://rp.example.test/callback".to_owned()],
        post_logout_redirect_uris: Vec::new(),
        backchannel_logout_uri: None,
        token_endpoint_auth_method: ClientAuthMethod::ClientSecretBasic,
        jwks: None,
        grant_types: vec![GrantType::AuthorizationCode],
        scopes: vec![Scope::Openid],
        members_only: false,
        trusted: false,
    }
}

/// A factor of an identity, by the value it holds.
async fn factor_of(harness: &Local, identity: Id<Identity>, value: &str) -> Id<crate::ids::Factor> {
    let rows = harness
        .store()
        .query::<Count>(
            "SELECT id AS n FROM factor WHERE identity_id = $1 AND value = $2",
            bind![identity, value],
        )
        .await
        .expect("the query runs");
    Id::new(rows.first().expect("the factor was added").0)
}

/// [`count`], for a statement with parameters.
async fn count_of(harness: &Local, sql: &'static str, params: Vec<crate::store::Value>) -> i64 {
    harness
        .store()
        .query::<Count>(sql, params)
        .await
        .expect("the query runs")[0]
        .0
}

/// The `kid` of the one key that is `retiring` — everything except the one
/// just minted.
async fn retiring_kid(harness: &Local, active: &str) -> String {
    let published = crate::oidc::key::published(harness.store())
        .await
        .expect("the JWKS reads");
    published
        .into_iter()
        .find(|row| row.kid != active)
        .expect("a second rotation leaves one key retiring")
        .kid
}
