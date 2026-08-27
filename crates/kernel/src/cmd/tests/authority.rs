//! Who may take something away, and who a command is attributed to.
//!
//! Three stories that are all about authority rather than about creation:
//! withdrawing an invitation nobody claimed, removing somebody else's
//! membership, and speaking as an organization. The last is the one that is
//! *not* about authority and says so — `ActAs` moves attribution and leaves
//! every authorisation where it was.

use rn_api::commands::{ClaimLink, RemoveMember, RevokeLink, SetRole};
use rn_api::whoami::MemberRole as WireRole;

use super::world::{self, key, public};
use crate::bind;
use crate::cmd;
use crate::event::Event;
use crate::ids::Id;
use crate::org::{self, MemberRole};
use crate::relation::{self, Object};
use crate::testing::{Local, TEST_EPOCH};

// ------------------------------------------------------- revoking a link ---

#[tokio::test]
async fn an_admin_withdraws_an_unclaimed_invitation_and_the_token_stops_being_worth_anything() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "rl-admin@example.test").await;
    let outsider = world::person(&harness, "Outsider", "rl-outsider@example.test").await;
    let group = world::group(&harness, &ronit, "Team", None)
        .await
        .expect("creates");
    let token = world::invite(
        &harness,
        &ronit,
        public(key(&harness), group),
        WireRole::Member,
        TEST_EPOCH + 3_600,
    )
    .await
    .expect("mints");

    let link = crate::invite::lookup(harness.store(), token.digest())
        .await
        .expect("reads")
        .expect("the token is worth something")
        .id();
    let args = RevokeLink {
        link: public(key(&harness), link),
    };

    // Somebody who administers nothing here is refused, and refused the way a
    // forged id would be.
    let refused = cmd::revoke_link(&harness.ctx(outsider.principal.clone()), &args).await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));

    let committed = cmd::revoke_link(&harness.ctx(ronit.principal.clone()), &args)
        .await
        .expect("its minter administers the group");
    assert_eq!(
        committed.event,
        Event::LinkRevoked {
            container: group,
            link
        }
    );

    // Both rows are gone, so the token opens nothing and the grant it carried
    // is not a dangling row somebody could still claim through.
    assert!(
        crate::invite::lookup(harness.store(), token.digest())
            .await
            .expect("reads")
            .is_none()
    );
    assert!(
        relation::list_for_object(harness.store(), Object::group(group))
            .await
            .expect("lists")
            .is_empty()
    );

    // And a second attempt on the same link is the same decline as one on an
    // id that never existed.
    let again = cmd::revoke_link(&harness.ctx(ronit.principal), &args).await;
    assert!(matches!(again, Err(ref e) if e.is_decline()));
}

#[tokio::test]
async fn an_invitation_somebody_already_claimed_is_not_withdrawn() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "rl-claimed@example.test").await;
    let joiner = world::person(&harness, "Joiner", "rl-joiner@example.test").await;
    let group = world::group(&harness, &ronit, "Team", None)
        .await
        .expect("creates");
    let token = world::invite(
        &harness,
        &ronit,
        public(key(&harness), group),
        WireRole::Member,
        TEST_EPOCH + 3_600,
    )
    .await
    .expect("mints");
    let link = crate::invite::lookup(harness.store(), token.digest())
        .await
        .expect("reads")
        .expect("worth something")
        .id();

    cmd::claim_link(
        &harness.ctx(joiner.principal.clone()),
        &ClaimLink {
            token: token.expose().to_owned(),
        },
    )
    .await
    .expect("claims");

    // Withdrawing it now would say nothing about the membership it produced,
    // and would take the claim the merge lane reads as a signal.
    let refused = cmd::revoke_link(
        &harness.ctx(ronit.principal),
        &RevokeLink {
            link: public(key(&harness), link),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));
    assert_eq!(
        org::role_of(harness.store(), Id::new(group.get()), joiner.person())
            .await
            .expect("reads"),
        Some(MemberRole::Member)
    );
}

// ------------------------------------------------------ removing somebody ---

#[tokio::test]
async fn an_admin_removes_a_role_below_their_own_and_never_the_last_owner() {
    let harness = Local::new();
    let owner = world::person(&harness, "Owner", "rm-owner@example.test").await;
    let admin = world::person(&harness, "Admin", "rm-admin@example.test").await;
    let joiner = world::person(&harness, "Joiner", "rm-joiner@example.test").await;
    let group = world::group(&harness, &owner, "Team", None)
        .await
        .expect("creates");
    let container = public(key(&harness), group);
    let party = Id::new(group.get());

    for who in [&admin, &joiner] {
        let token = world::invite(
            &harness,
            &owner,
            container.clone(),
            WireRole::Member,
            TEST_EPOCH + 3_600,
        )
        .await
        .expect("mints");
        cmd::claim_link(
            &harness.ctx(who.principal.clone()),
            &ClaimLink {
                token: token.expose().to_owned(),
            },
        )
        .await
        .expect("claims");
    }
    cmd::set_role(
        &harness.ctx(owner.principal.clone()),
        &SetRole {
            group: container.clone(),
            party: public(key(&harness), admin.person()),
            role: WireRole::Admin,
        },
    )
    .await
    .expect("promotes");

    // A member removes nobody, however much they would like to.
    let refused = cmd::remove_member(
        &harness.ctx(joiner.principal.clone()),
        &RemoveMember {
            group: container.clone(),
            party: public(key(&harness), admin.person()),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));

    // An admin removes a member — and not the owner above them, nor another
    // admin, which would be removing whoever appointed them.
    for above in [owner.person(), admin.person()] {
        let refused = cmd::remove_member(
            &harness.ctx(admin.principal.clone()),
            &RemoveMember {
                group: container.clone(),
                party: public(key(&harness), above),
            },
        )
        .await;
        assert!(matches!(refused, Err(ref e) if e.is_decline()));
    }
    let committed = cmd::remove_member(
        &harness.ctx(admin.principal.clone()),
        &RemoveMember {
            group: container.clone(),
            party: public(key(&harness), joiner.person()),
        },
    )
    .await
    .expect("an admin removes a member");
    assert_eq!(
        committed.event,
        Event::MemberRemoved {
            container: group,
            party: joiner.person()
        }
    );
    assert_eq!(
        org::role_of(harness.store(), party, joiner.person())
            .await
            .expect("reads"),
        None
    );

    // The owner removes the admin, and cannot remove themselves — that is
    // `Leave`, and here it is the last owner anyway.
    cmd::remove_member(
        &harness.ctx(owner.principal.clone()),
        &RemoveMember {
            group: container.clone(),
            party: public(key(&harness), admin.person()),
        },
    )
    .await
    .expect("an owner removes an admin");
    let me = public(key(&harness), owner.person());
    let refused = cmd::remove_member(
        &harness.ctx(owner.principal.clone()),
        &RemoveMember {
            group: container,
            party: me,
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));
    assert_eq!(
        org::owner_count(harness.store(), party)
            .await
            .expect("reads"),
        1
    );
}

// ------------------------------------------------------------- acting as ---

#[tokio::test]
async fn a_session_speaks_as_an_organization_it_administers_and_as_no_other() {
    use rn_api::commands::{ActAs, CreateDocument};

    let harness = Local::new();
    let founder = world::person(&harness, "Founder", "aa-founder@example.test").await;
    let stranger = world::person(&harness, "Stranger", "aa-stranger@example.test").await;
    let (org, _) = world::organization(&harness, &founder, "Isoastra")
        .await
        .expect("founds");
    let organization = public(key(&harness), org);

    // Somebody else's organization is not a party you may speak as, and a
    // group never is: a group is granted things and never acts.
    let refused = cmd::act_as(
        &harness.ctx(stranger.principal.clone()),
        &ActAs {
            party: Some(organization.clone()),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));
    let team = world::group(&harness, &founder, "Team", Some(org))
        .await
        .expect("creates");
    let refused = cmd::act_as(
        &harness.ctx(founder.principal.clone()),
        &ActAs {
            party: Some(public(key(&harness), team)),
        },
    )
    .await;
    assert!(matches!(refused, Err(ref e) if e.is_decline()));

    let committed = cmd::act_as(
        &harness.ctx(founder.principal.clone()),
        &ActAs {
            party: Some(organization),
        },
    )
    .await
    .expect("its admin may");
    let session = founder.principal.session().expect("a member has a session");
    assert_eq!(
        committed.event,
        Event::ActingAs {
            identity: founder
                .principal
                .identity()
                .expect("a member has an identity"),
            session,
            party: Id::new(org.get()),
        }
    );

    // The switch is on the row, so the next request resolves to it.
    assert_eq!(
        world::count(
            &harness,
            "SELECT count(*) AS n FROM session WHERE id = $1 AND acting_as = $2",
            bind![session, Id::<crate::ids::Person>::new(org.get())],
        )
        .await,
        1,
        "the session still speaks as the person"
    );

    // And a command run on that principal is attributed to the organization
    // while its authority is still the person's — the document is created,
    // and the audit row says the organization did it.
    let acting_principal = crate::principal::Principal::Member {
        identity: founder.principal.identity().expect("has one"),
        person: Some(founder.person()),
        acting_as: Id::new(org.get()),
        session,
        impersonated_by: None,
    };
    cmd::create_document(
        &harness.ctx(acting_principal),
        &CreateDocument {
            title: "Charter".into(),
            body: String::new(),
            owner: None,
        },
    )
    .await
    .expect("the person's own authority still applies");
    assert_eq!(
        world::count(
            &harness,
            "SELECT count(*) AS n FROM audit WHERE command = 'create-document' AND acting_as = $1",
            bind![Id::<crate::ids::Person>::new(org.get())],
        )
        .await,
        1,
        "the audit row did not record the organization"
    );
}

// ------------------------------------------------------ operator reach ---

#[tokio::test]
async fn a_platform_operator_revokes_a_grant_and_transfers_a_resource_they_do_not_own() {
    use rn_api::commands::{Revoke, Share, Transfer};
    use rn_api::whoami::DocRole;

    let harness = Local::new();
    let owner = world::person(&harness, "Owner", "op-doc-owner@example.test").await;
    let reader = world::person(&harness, "Reader", "op-doc-reader@example.test").await;
    let operator = world::person(&harness, "Operator", "op-doc-operator@example.test").await;
    let document = world::document(&harness, &owner, "Charter", None)
        .await
        .expect("creates");
    let resource = public(key(&harness), document);
    cmd::share(
        &harness.ctx(owner.principal.clone()),
        &Share {
            resource: resource.clone(),
            subject: public(key(&harness), reader.person()),
            relation: DocRole::Viewer,
        },
    )
    .await
    .expect("shares");

    let revoke = Revoke {
        resource: resource.clone(),
        subject: public(key(&harness), reader.person()),
        relation: DocRole::Viewer,
    };
    let transfer = Transfer {
        resource: resource.clone(),
        to: public(key(&harness), operator.person()),
    };

    // A stranger with no relation on it reaches neither.
    assert!(matches!(
        cmd::revoke(&harness.ctx(operator.principal.clone()), &revoke).await,
        Err(ref e) if e.is_decline()
    ));
    assert!(matches!(
        cmd::transfer(&harness.ctx(operator.principal.clone()), &transfer).await,
        Err(ref e) if e.is_decline()
    ));

    super::prelude::make_operator(&harness, operator.person()).await;

    cmd::revoke(&harness.ctx(operator.principal.clone()), &revoke)
        .await
        .expect("an operator may withdraw any relation");
    assert!(
        !relation::list_for_object(harness.store(), Object::resource("document", document))
            .await
            .expect("lists")
            .iter()
            .any(|grant| grant.relation == crate::relation::Relation::Viewer)
    );

    cmd::transfer(&harness.ctx(operator.principal.clone()), &transfer)
        .await
        .expect("an operator may reassign any resource");
    assert_eq!(
        crate::resource::load(harness.store(), document)
            .await
            .expect("reads")
            .expect("still there")
            .owner_party_id,
        operator.person()
    );

    // Both are on the record under the operator's own identity, which is what
    // makes the reach reviewable rather than merely present.
    assert_eq!(
        world::count(
            &harness,
            "SELECT count(*) AS n FROM audit \
             WHERE command IN ('revoke', 'transfer') AND acting_as = $1",
            bind![operator.person()],
        )
        .await,
        2
    );
}
