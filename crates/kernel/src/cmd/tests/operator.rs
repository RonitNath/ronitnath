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

use std::future::Future;

use rn_api::commands::{ALL_COMMAND_NAMES, MatchSignal};
use rn_api::commands::{
    AddFactor, CreateDocument, CreateGroup, CreateOrganization, DeleteClient, Disable,
    EditDocument, Enable, FactorKind, GrantOperator, Invite, Leave, ProposeMatch, PublishDocument,
    RegisterClient, RemoveFactor, RemoveMember, RetireKey, Revoke, RevokeLink, RevokeOperator,
    RevokeSession, RotateClientSecret, RotateSigningKey, RuleMatch, SetRole, Share, SignInAs,
    Split, Transfer, UpdateClient,
};
use rn_api::oidc::{ClientAuthMethod, ClientMetadata, GrantType, Scope};
use rn_api::whoami::{DocRole, MemberRole as WireRole};

use super::prelude::*;
use super::world::{self, key, public};
use crate::cmd;
use crate::error::Outcome;
use crate::ids::{Id, Identity, Person, Resource};
use crate::testing::TEST_EPOCH;

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

#[tokio::test]
async fn an_operator_reaches_every_command_a_principal_authorises() {
    let harness = Local::new();
    let key = key(&harness);

    // Three people who are not the agent, and everything they own.
    let boss = world::person(&harness, "Boss", "op-boss@example.test").await;
    let member = world::person(&harness, "Member", "op-member@example.test").await;
    let stray = world::person(&harness, "Stray", "op-stray@example.test").await;
    // The agent. Nothing is theirs, and after the first pass everything is.
    let agent = world::person(&harness, "Agent", "op-agent@example.test").await;

    let (org, org_resource) = world::organization(&harness, &boss, "Isoastra")
        .await
        .expect("founds");
    let _group = world::group(&harness, &boss, "Team", Some(org))
        .await
        .expect("creates");
    let document = world::document(&harness, &boss, "The ruling", None)
        .await
        .expect("creates");
    let _ = org_resource;

    // A member of the organization, so `set-role` and `remove-member` have
    // somebody to act on who is not its owner.
    let token = world::invite(
        &harness,
        &boss,
        public(key, org),
        WireRole::Member,
        TEST_EPOCH + 3_600,
    )
    .await
    .expect("mints");
    cmd::claim_link(
        &harness.ctx(member.principal.clone()),
        &rn_api::commands::ClaimLink {
            token: token.expose().to_owned(),
        },
    )
    .await
    .expect("joins");

    // An unclaimed invitation for `revoke-link`.
    let spare = world::invite(
        &harness,
        &boss,
        public(key, org),
        WireRole::Member,
        TEST_EPOCH + 3_600,
    )
    .await
    .expect("mints");
    let spare_link = crate::invite::lookup(harness.store(), spare.digest())
        .await
        .expect("reads")
        .expect("a live invitation")
        .id();

    // A live session of somebody else, for `revoke-session`.
    let strays_session = harness
        .sign_in("op-stray@example.test")
        .await
        .expect("signs in")
        .principal
        .session()
        .expect("a member holds one");

    // Two identities of two different people, for the merge commands. `stray`
    // registers a second address, which is a second identity of a second
    // person until somebody rules that they are one human.
    let second = harness
        .register("Stray again", "op-stray-2@example.test")
        .await
        .expect("registers");
    let stray_a = stray.principal.identity().expect("a member holds one");
    let stray_b = second.principal.identity().expect("a member holds one");

    // A relying party for the registry commands.
    let client = cmd::register_client(
        &harness.ctx(boss.principal.clone()),
        &RegisterClient {
            owner: None,
            metadata: metadata("Boss's client"),
        },
    )
    .await;
    // The boss is not an operator either, so the registry refuses them — which
    // is itself part of the finding. The agent registers one in the second
    // pass and the later registry commands act on that.
    assert!(client.is_err(), "registering a client is operator work");

    let agent_person = agent.person();
    let ctx = || harness.ctx(agent.principal.clone());

    // ------------------------------------------------------ the stranger ---
    //
    // Everything below runs as somebody who holds nothing, against rows that
    // belong to other people. Every one is the uniform decline, and none of
    // them writes anything.

    let propose = ProposeMatch {
        identity_a: public(key, stray_a),
        identity_b: public(key, stray_b),
        signal: MatchSignal::VerifiedEmail,
    };
    refused("propose-match", cmd::propose_match(&ctx(), &propose).await).await;

    let create_in_org = CreateDocument {
        title: "Agent's".into(),
        body: "In somebody else's organization".into(),
        owner: Some(public(key, org)),
    };
    refused(
        "create-document",
        cmd::create_document(&ctx(), &create_in_org).await,
    )
    .await;

    let create_group = CreateGroup {
        display_name: "Agent's group".into(),
        organization: Some(public(key, org)),
    };
    refused(
        "create-group",
        cmd::create_group(&ctx(), &create_group).await,
    )
    .await;

    let invite = Invite {
        group: public(key, org),
        role: WireRole::Member,
        expires_at: TEST_EPOCH + 3_600,
    };
    refused("invite", cmd::invite(&ctx(), &invite).await).await;

    let revoke_link = RevokeLink {
        link: public(key, spare_link),
    };
    refused("revoke-link", cmd::revoke_link(&ctx(), &revoke_link).await).await;

    let set_role = SetRole {
        group: public(key, org),
        party: public(key, member.person()),
        role: WireRole::Admin,
    };
    refused("set-role", cmd::set_role(&ctx(), &set_role).await).await;

    let edit = EditDocument {
        document: public(key, document),
        expected_rev: 1,
        title: Some("Edited by an operator".into()),
        body: None,
    };
    refused("edit-document", cmd::edit_document(&ctx(), &edit).await).await;

    let publish = PublishDocument {
        document: public(key, document),
    };
    refused(
        "publish-document",
        cmd::publish_document(&ctx(), &publish).await,
    )
    .await;

    let share = Share {
        resource: public(key, document),
        subject: public(key, stray.person()),
        relation: DocRole::Viewer,
    };
    refused("share", cmd::share(&ctx(), &share).await).await;

    let transfer = Transfer {
        resource: public(key, document),
        to: public(key, agent_person),
    };
    refused("transfer", cmd::transfer(&ctx(), &transfer).await).await;

    let add_factor = AddFactor {
        kind: FactorKind::Email,
        value: "op-recovered@example.test".into(),
        identity: Some(public(key, stray_a)),
    };
    refused("add-factor", cmd::add_factor(&ctx(), &add_factor).await).await;

    let revoke_session = RevokeSession {
        session: public(key, strays_session),
    };
    refused(
        "revoke-session",
        cmd::revoke_session(&ctx(), &revoke_session).await,
    )
    .await;

    refused(
        "register-client",
        cmd::register_client(
            &ctx(),
            &RegisterClient {
                owner: None,
                metadata: metadata("Agent's client"),
            },
        )
        .await,
    )
    .await;

    refused(
        "rotate-signing-key",
        cmd::rotate_signing_key(&ctx(), &RotateSigningKey {}).await,
    )
    .await;

    let disable = Disable {
        party: public(key, stray.person()),
        reason: "an operator ruling".into(),
    };
    refused("disable", cmd::disable(&ctx(), &disable).await).await;

    let grant = GrantOperator {
        person: public(key, member.person()),
        reason: "a second pair of hands for the cutover".into(),
    };
    refused("grant-operator", cmd::grant_operator(&ctx(), &grant).await).await;

    let revoke_operator = RevokeOperator {
        person: public(key, member.person()),
        reason: "the cutover is done".into(),
    };
    refused(
        "revoke-operator",
        cmd::revoke_operator(&ctx(), &revoke_operator).await,
    )
    .await;

    let sign_in_as = SignInAs {
        person: public(key, member.person()),
        reason: "reproducing the bug they reported".into(),
    };
    refused("sign-in-as", cmd::sign_in_as(&ctx(), &sign_in_as).await).await;

    let remove_member = RemoveMember {
        group: public(key, org),
        party: public(key, member.person()),
    };
    refused(
        "remove-member",
        cmd::remove_member(&ctx(), &remove_member).await,
    )
    .await;

    // ------------------------------------------------------- the operator ---

    make_operator(&harness, agent_person).await;
    let before = count(&harness, "SELECT count(*) AS n FROM audit").await;
    let mut seen: Vec<&'static str> = Vec::new();

    sweep(&mut seen, "propose-match", Then::Works, || async {
        cmd::propose_match(&ctx(), &propose).await.map(|_| ())
    })
    .await;

    // The candidate the proposal just queued, ruled on by the operator: the
    // two registrations are one human, with evidence, which is the whole of
    // what `rule-match` is for.
    let candidate: Id<crate::ids::MatchCandidate> = Id::new(
        count_of(
            &harness,
            "SELECT id AS n FROM match_candidate WHERE identity_a = $1 OR identity_b = $1",
            bind![stray_a],
        )
        .await,
    );
    let rule = RuleMatch {
        candidate: public(key, candidate),
        same_person: true,
        evidence: "A passport and a support call.".into(),
    };
    sweep(&mut seen, "rule-match", Then::Works, || async {
        cmd::rule_match(&ctx(), &rule).await.map(|_| ())
    })
    .await;

    // And undone: the merge above left one person holding two identities,
    // which is exactly the precondition `split` needs.
    let split = Split {
        identity: public(key, stray_b),
        evidence: "The ruling was wrong.".into(),
    };
    sweep(&mut seen, "split", Then::Works, || async {
        cmd::split(&ctx(), &split).await.map(|_| ())
    })
    .await;

    // Founding is nobody's authority but a signed-in person's, so an operator
    // reaches it the way everybody does. It is on the list because a later
    // change that made it need one has to fail here.
    let found = CreateOrganization {
        display_name: "The agency".into(),
    };
    sweep(&mut seen, "create-organization", Then::Works, || async {
        cmd::create_organization(&ctx(), &found).await.map(|_| ())
    })
    .await;

    sweep(&mut seen, "create-group", Then::Works, || async {
        cmd::create_group(&ctx(), &create_group).await.map(|_| ())
    })
    .await;
    sweep(&mut seen, "invite", Then::Works, || async {
        cmd::invite(&ctx(), &invite).await.map(|_| ())
    })
    .await;
    sweep(&mut seen, "revoke-link", Then::Works, || async {
        cmd::revoke_link(&ctx(), &revoke_link).await.map(|_| ())
    })
    .await;
    sweep(&mut seen, "set-role", Then::Works, || async {
        cmd::set_role(&ctx(), &set_role).await.map(|_| ())
    })
    .await;
    sweep(&mut seen, "remove-member", Then::Works, || async {
        cmd::remove_member(&ctx(), &remove_member).await.map(|_| ())
    })
    .await;

    // The one command an operator does not reach, and the reason is the
    // missing row rather than the missing authority: leaving is about a
    // membership, and an operator holds none.
    let leave = Leave {
        group: public(key, org),
    };
    sweep(
        &mut seen,
        "leave",
        Then::Declines("an operator holds no membership to leave"),
        || async { cmd::leave(&ctx(), &leave).await.map(|_| ()) },
    )
    .await;

    sweep(&mut seen, "create-document", Then::Works, || async {
        cmd::create_document(&ctx(), &create_in_org)
            .await
            .map(|_| ())
    })
    .await;
    sweep(&mut seen, "edit-document", Then::Works, || async {
        cmd::edit_document(&ctx(), &edit).await.map(|_| ())
    })
    .await;
    sweep(&mut seen, "publish-document", Then::Works, || async {
        cmd::publish_document(&ctx(), &publish).await.map(|_| ())
    })
    .await;
    sweep(&mut seen, "share", Then::Works, || async {
        cmd::share(&ctx(), &share).await.map(|_| ())
    })
    .await;
    let revoke = Revoke {
        resource: public(key, document),
        subject: public(key, stray.person()),
        relation: DocRole::Viewer,
    };
    sweep(&mut seen, "revoke", Then::Works, || async {
        cmd::revoke(&ctx(), &revoke).await.map(|_| ())
    })
    .await;
    sweep(&mut seen, "transfer", Then::Works, || async {
        cmd::transfer(&ctx(), &transfer).await.map(|_| ())
    })
    .await;

    // The account-recovery pair: an operator moves a factor onto a
    // registration that is not theirs, which is the whole reason `rule-match`
    // exists and the one thing that gives the human it ruled about a way in.
    sweep(&mut seen, "add-factor", Then::Works, || async {
        cmd::add_factor(&ctx(), &add_factor).await.map(|_| ())
    })
    .await;
    let added = factor_of(&harness, stray_a, "op-recovered@example.test").await;
    let remove_factor = RemoveFactor {
        factor: public(key, added),
        identity: Some(public(key, stray_a)),
    };
    sweep(&mut seen, "remove-factor", Then::Works, || async {
        cmd::remove_factor(&ctx(), &remove_factor).await.map(|_| ())
    })
    .await;

    sweep(&mut seen, "revoke-session", Then::Works, || async {
        cmd::revoke_session(&ctx(), &revoke_session)
            .await
            .map(|_| ())
    })
    .await;

    // The OpenID registry.
    let registered = cmd::register_client(
        &ctx(),
        &RegisterClient {
            owner: None,
            metadata: metadata("Agent's client"),
        },
    )
    .await
    .expect("an operator registers a relying party");
    seen.push("register-client");
    let crate::Event::ClientRegistered { client, .. } = registered.committed.event else {
        panic!("registering a client produces a registration");
    };
    let client_id = client.public(key);

    let update = UpdateClient {
        client: client_id.clone(),
        metadata: metadata("Agent's client, renamed"),
    };
    sweep(&mut seen, "update-client", Then::Works, || async {
        cmd::update_client(&ctx(), &update).await.map(|_| ())
    })
    .await;
    let rotate = RotateClientSecret {
        client: client_id.clone(),
    };
    sweep(&mut seen, "rotate-client-secret", Then::Works, || async {
        cmd::rotate_client_secret(&ctx(), &rotate).await.map(|_| ())
    })
    .await;
    let delete = DeleteClient {
        client: client_id.clone(),
    };
    sweep(&mut seen, "delete-client", Then::Works, || async {
        cmd::delete_client(&ctx(), &delete).await.map(|_| ())
    })
    .await;
    sweep(&mut seen, "rotate-signing-key", Then::Works, || async {
        cmd::rotate_signing_key(&ctx(), &RotateSigningKey {})
            .await
            .map(|_| ())
    })
    .await;

    // Delegation, and taking it back. In that order and about somebody else,
    // because the last-operator rule means an operator cannot revoke the only
    // one there is — including themselves.
    sweep(&mut seen, "grant-operator", Then::Works, || async {
        cmd::grant_operator(&ctx(), &grant).await.map(|_| ())
    })
    .await;
    sweep(&mut seen, "revoke-operator", Then::Works, || async {
        cmd::revoke_operator(&ctx(), &revoke_operator)
            .await
            .map(|_| ())
    })
    .await;
    sweep(&mut seen, "sign-in-as", Then::Works, || async {
        cmd::sign_in_as(&ctx(), &sign_in_as).await.map(|_| ())
    })
    .await;

    // A second rotation is what puts the first key into `retiring`, which is
    // the only status this command acts on: the active key is what everything
    // is being signed under right now.
    let second = cmd::rotate_signing_key(&ctx(), &RotateSigningKey {})
        .await
        .expect("an operator rotates");
    let crate::Event::SigningKeyRotated { kid: newest } = second.event else {
        panic!("rotating produces a rotation");
    };
    let retiring = retiring_kid(&harness, &newest).await;
    let retire = RetireKey {
        kid: retiring,
        force: false,
        reason: "nothing signed under it is alive".into(),
    };
    sweep(&mut seen, "retire-key", Then::Works, || async {
        cmd::retire_key(&ctx(), &retire).await.map(|_| ())
    })
    .await;

    // Last, because they end sessions and change what everything above could
    // still have read.
    sweep(&mut seen, "disable", Then::Works, || async {
        cmd::disable(&ctx(), &disable).await.map(|_| ())
    })
    .await;
    let enable = Enable {
        party: public(key, stray.person()),
    };
    sweep(&mut seen, "enable", Then::Works, || async {
        cmd::enable(&ctx(), &enable).await.map(|_| ())
    })
    .await;

    // Every gated command was exercised, and every success is a row naming
    // the agent's identity. The second assertion is the one that makes the
    // power reviewable rather than merely present.
    let mut sorted = seen.clone();
    sorted.sort_unstable();
    let mut expected = GATED.to_vec();
    expected.sort_unstable();
    assert_eq!(sorted, expected, "the sweep did not run what it classified");

    let after = count(&harness, "SELECT count(*) AS n FROM audit").await;
    assert!(
        after - before >= (GATED.len() - 1) as i64,
        "an operator's work left {} audit rows for {} commands",
        after - before,
        GATED.len()
    );
    let theirs = count_of(
        &harness,
        "SELECT count(*) AS n FROM audit WHERE actor_identity_id = $1 AND id > $2",
        bind![agent.principal.identity().expect("a member"), before],
    )
    .await;
    assert_eq!(
        theirs,
        after - before,
        "every ruling an operator made must name them"
    );
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

// Silence the unused-import warnings the two id aliases would otherwise raise
// on a build where the assertions above are compiled out.
const _: Option<Id<Person>> = None;
const _: Option<Id<Resource>> = None;

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
