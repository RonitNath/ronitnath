//! Organizations, groups, invitations and roles.

use rn_api::commands::{ClaimLink, Leave, RemoveMember, RevokeLink, SetRole};
use rn_api::whoami::MemberRole as WireRole;

use super::world::{self, key, public};
use crate::bind;
use crate::cmd;
use crate::event::Event;
use crate::ids::Id;
use crate::org::{self, MemberRole};
use crate::relation::{self, Object, Relation, Subject};
use crate::resource;
use crate::testing::{Local, TEST_EPOCH};

#[tokio::test]
async fn create_organization_makes_a_party_a_resource_a_join_and_an_owner() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "org@example.test").await;
    let (org, resource_id) = world::organization(&harness, &ronit, "Isoastra")
        .await
        .expect("founds");

    let row = resource::of_organization(harness.store(), org)
        .await
        .expect("reads")
        .expect("the resource is there");
    assert_eq!(row.id, resource_id);
    assert_eq!(row.kind, resource::kinds::ORGANIZATION);
    assert_eq!(
        row.owner_party_id,
        ronit.person(),
        "the founder owns it, and only Transfer moves that"
    );
    assert_eq!(
        org::role_of(harness.store(), Id::new(org.get()), ronit.person())
            .await
            .expect("reads"),
        Some(MemberRole::Owner)
    );
}

#[tokio::test]
async fn a_group_inside_an_organization_needs_admin_there() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "g-owner@example.test").await;
    let abeer = world::person(&harness, "Abeer", "g-member@example.test").await;
    let (org, _) = world::organization(&harness, &ronit, "Isoastra")
        .await
        .expect("founds");

    // A stranger to the organization cannot make a group in it.
    let refused = world::group(&harness, &abeer, "Founders", Some(org)).await;
    assert!(refused.is_err_and(|err| err.is_decline()));

    let group = world::group(&harness, &ronit, "Founders", Some(org))
        .await
        .expect("the owner may");
    let owning = crate::group::load(harness.store(), group)
        .await
        .expect("reads")
        .expect("the group is there");
    assert_eq!(owning.organization(), Some(org));
    assert!(!owning.is_personal());
}

#[tokio::test]
async fn a_group_with_no_organization_belongs_to_the_person_who_made_it() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "contacts@example.test").await;
    let group = world::group(&harness, &ronit, "Contacts", None)
        .await
        .expect("creates");
    let row = crate::group::load(harness.store(), group)
        .await
        .expect("reads")
        .expect("it is there");
    assert!(row.is_personal());
    assert_eq!(row.resource.owner_party_id, ronit.person());
    assert_eq!(row.organization(), None);
}

#[tokio::test]
async fn an_invitation_is_a_link_and_a_relation_row_and_the_token_comes_once() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "inv@example.test").await;
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

    let grants = relation::list_for_object(harness.store(), Object::group(group))
        .await
        .expect("lists");
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].relation, Relation::Member);
    assert_eq!(grants[0].subject.kind, relation::SubjectKind::Link);

    let invitation = crate::invite::lookup(harness.store(), token.digest())
        .await
        .expect("reads")
        .expect("the token is worth something");
    assert_eq!(invitation.container, group);
    assert_eq!(invitation.role, MemberRole::Member);
}

#[tokio::test]
async fn only_an_admin_invites_and_never_to_ownership() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "inv-owner@example.test").await;
    let abeer = world::person(&harness, "Abeer", "inv-none@example.test").await;
    let group = world::group(&harness, &ronit, "Team", None)
        .await
        .expect("creates");
    let id = public(key(&harness), group);

    let outsider = world::invite(
        &harness,
        &abeer,
        id.clone(),
        WireRole::Member,
        TEST_EPOCH + 60,
    )
    .await;
    assert!(outsider.is_err_and(|err| err.is_decline()));

    let ownership = world::invite(
        &harness,
        &ronit,
        id.clone(),
        WireRole::Owner,
        TEST_EPOCH + 60,
    )
    .await;
    assert!(
        ownership.is_err_and(|err| err.is_decline()),
        "ownership moves through Transfer and nowhere else"
    );

    let expired = world::invite(&harness, &ronit, id, WireRole::Member, TEST_EPOCH - 1).await;
    assert!(expired.is_err_and(|err| err.is_decline()));
}

#[tokio::test]
async fn a_link_is_claimed_once_and_then_never_again() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "claim-owner@example.test").await;
    let abeer = world::person(&harness, "Abeer", "claim-joiner@example.test").await;
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

    let committed = cmd::claim_link(
        &harness.ctx(abeer.principal.clone()),
        &ClaimLink {
            token: token.expose().to_owned(),
        },
    )
    .await
    .expect("claims");
    assert!(matches!(committed.event, Event::LinkClaimed { .. }));
    assert_eq!(
        crate::group::role_of(harness.store(), group, abeer.person())
            .await
            .expect("reads"),
        Some(MemberRole::Member)
    );

    // A second claim — a different key, the same token — is refused.
    let again = cmd::claim_link(
        &harness.ctx(abeer.principal.clone()),
        &ClaimLink {
            token: token.expose().to_owned(),
        },
    )
    .await;
    assert!(again.is_err_and(|err| err.is_decline()));
    assert_eq!(
        world::count(
            &harness,
            "SELECT count(*) AS n FROM membership WHERE group_id = $1",
            bind![group]
        )
        .await,
        2,
        "the owner and the one person who claimed"
    );
}

#[tokio::test]
async fn an_expired_link_opens_nothing() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "exp-owner@example.test").await;
    let abeer = world::person(&harness, "Abeer", "exp-joiner@example.test").await;
    let group = world::group(&harness, &ronit, "Team", None)
        .await
        .expect("creates");
    let token = world::invite(
        &harness,
        &ronit,
        public(key(&harness), group),
        WireRole::Member,
        TEST_EPOCH + 60,
    )
    .await
    .expect("mints");

    harness.advance(120);
    let refused = cmd::claim_link(
        &harness.ctx(abeer.principal.clone()),
        &ClaimLink {
            token: token.expose().to_owned(),
        },
    )
    .await;
    assert!(refused.is_err_and(|err| err.is_decline()));
}

#[tokio::test]
async fn set_role_moves_a_member_and_refuses_the_owner_seat() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "role-owner@example.test").await;
    let abeer = world::person(&harness, "Abeer", "role-member@example.test").await;
    let group = world::group(&harness, &ronit, "Team", None)
        .await
        .expect("creates");
    join(&harness, &ronit, &abeer, group).await;

    let group_id = public(key(&harness), group);
    let party = public(key(&harness), abeer.person());
    cmd::set_role(
        &harness.ctx(ronit.principal.clone()),
        &SetRole {
            group: group_id.clone(),
            party: party.clone(),
            role: WireRole::Admin,
        },
    )
    .await
    .expect("promotes");
    assert_eq!(
        crate::group::role_of(harness.store(), group, abeer.person())
            .await
            .expect("reads"),
        Some(MemberRole::Admin)
    );

    let ownership = cmd::set_role(
        &harness.ctx(ronit.principal.clone()),
        &SetRole {
            group: group_id,
            party,
            role: WireRole::Owner,
        },
    )
    .await;
    assert!(ownership.is_err_and(|err| err.is_decline()));
}

#[tokio::test]
async fn the_last_owner_can_neither_be_demoted_nor_leave() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "last-owner@example.test").await;
    let abeer = world::person(&harness, "Abeer", "last-member@example.test").await;
    let group = world::group(&harness, &ronit, "Team", None)
        .await
        .expect("creates");
    join(&harness, &ronit, &abeer, group).await;
    let group_id = public(key(&harness), group);

    let demoted = cmd::set_role(
        &harness.ctx(ronit.principal.clone()),
        &SetRole {
            group: group_id.clone(),
            party: public(key(&harness), ronit.person()),
            role: WireRole::Member,
        },
    )
    .await;
    assert!(demoted.is_err_and(|err| err.is_decline()));

    let left = cmd::leave(
        &harness.ctx(ronit.principal.clone()),
        &Leave {
            group: group_id.clone(),
        },
    )
    .await;
    assert!(
        left.is_err_and(|err| err.is_decline()),
        "a container with no owner is one nobody can administer"
    );

    // The member, on the other hand, may leave whenever they like.
    cmd::leave(
        &harness.ctx(abeer.principal.clone()),
        &Leave { group: group_id },
    )
    .await
    .expect("leaves");
    assert_eq!(
        crate::group::role_of(harness.store(), group, abeer.person())
            .await
            .expect("reads"),
        None
    );
}

/// Put somebody in a group through the invitation path, which is the only way
/// a membership is ever written.
async fn join(
    harness: &Local,
    owner: &world::Who,
    joiner: &world::Who,
    group: Id<crate::ids::Group>,
) {
    let token = world::invite(
        harness,
        owner,
        public(key(harness), group),
        WireRole::Member,
        TEST_EPOCH + 3_600,
    )
    .await
    .expect("mints");
    cmd::claim_link(
        &harness.ctx(joiner.principal.clone()),
        &ClaimLink {
            token: token.expose().to_owned(),
        },
    )
    .await
    .expect("claims");
}

#[tokio::test]
async fn a_relation_the_vocabulary_refuses_never_reaches_the_table() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "vocab@example.test").await;
    let group = world::group(&harness, &ronit, "Team", None)
        .await
        .expect("creates");
    // `person #contact @group` is admitted; `person #editor @group` is not,
    // and the registry is what says so before any row is written.
    assert!(relation::Vocabulary::KERNEL.admits(
        "person",
        Relation::Contact,
        relation::SubjectKind::Group
    ));
    assert!(!relation::Vocabulary::KERNEL.admits(
        "person",
        Relation::Editor,
        relation::SubjectKind::Group
    ));
    // And a relation the schema does not know is refused by the trigger even
    // when a caller reaches the store directly.
    assert_eq!(MemberRole::Member.relation(), Relation::Member);
    let subject = Subject::group(group);
    relation::grant(
        harness.store(),
        Object::person(ronit.person()),
        Relation::Contact,
        subject,
    )
    .await
    .expect("a contact grant is admitted");
}

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
