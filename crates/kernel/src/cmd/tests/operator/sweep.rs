//! The sweep itself: one world, two passes over it.
//!
//! The stranger runs every gated command against rows that belong to other
//! people and is refused everywhere, writing nothing — so the world the second
//! pass sees is the world the first pass found. Then the operator relation is
//! granted and the same commands run again, in an order chosen so each one's
//! precondition is still true.

use rn_api::commands::MatchSignal;
use rn_api::commands::{
    AddFactor, CreateDocument, CreateGroup, CreateOrganization, DeleteClient, Disable,
    EditDocument, Enable, FactorKind, GrantOperator, Invite, Leave, ProposeMatch, PublishDocument,
    RegisterClient, RemoveFactor, RemoveMember, RetireKey, Revoke, RevokeLink, RevokeOperator,
    RevokeSession, RotateClientSecret, RotateSigningKey, RuleMatch, SetRole, Share, SignInAs,
    Split, Transfer, UpdateClient,
};
use rn_api::whoami::{DocRole, MemberRole as WireRole};

use super::super::prelude::*;
use super::super::world::{self, key, public};
use super::{GATED, Then, count_of, factor_of, metadata, refused, retiring_kid, sweep};
use crate::cmd;
use crate::ids::Id;
use crate::testing::TEST_EPOCH;

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
