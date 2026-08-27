//! Sharing, revoking and transferring.

use rn_api::commands::{Revoke, Share, Transfer};
use rn_api::whoami::DocRole;

use super::world::{self, key, public};
use crate::cmd;
use crate::event::Event;
use crate::ids::Id;
use crate::principal::expand;
use crate::relation::{self, Object, Relation};
use crate::resource;
use crate::testing::Local;

#[tokio::test]
async fn an_owner_shares_a_viewer_cannot_and_a_revoke_takes_back_one_row() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "share-owner@example.test").await;
    let abeer = world::person(&harness, "Abeer", "share-reader@example.test").await;
    let doc = world::document(&harness, &ronit, "Kernel report", None)
        .await
        .expect("creates");
    let doc_id = public(key(&harness), doc);
    let reader = public(key(&harness), abeer.person());

    cmd::share(
        &harness.ctx(ronit.principal.clone()),
        &Share {
            resource: doc_id.clone(),
            subject: reader.clone(),
            relation: DocRole::Viewer,
        },
    )
    .await
    .expect("the owner may share");

    let seen = expand(harness.store(), &abeer.principal)
        .await
        .expect("expands");
    assert!(
        relation::check(
            harness.store(),
            &seen,
            Relation::Viewer,
            Object::document(doc)
        )
        .await
        .expect("checks")
    );

    // A viewer cannot hand their view on: a grant that made itself transitive
    // would be one the person who wrote it never agreed to.
    let onwards = cmd::share(
        &harness.ctx(abeer.principal.clone()),
        &Share {
            resource: doc_id.clone(),
            subject: reader.clone(),
            relation: DocRole::Editor,
        },
    )
    .await;
    assert!(onwards.is_err_and(|err| err.is_decline()));

    cmd::revoke(
        &harness.ctx(ronit.principal.clone()),
        &Revoke {
            resource: doc_id,
            subject: reader,
            relation: DocRole::Viewer,
        },
    )
    .await
    .expect("revokes");
    let after = expand(harness.store(), &abeer.principal)
        .await
        .expect("expands");
    assert!(
        !relation::check(
            harness.store(),
            &after,
            Relation::Viewer,
            Object::document(doc)
        )
        .await
        .expect("checks")
    );
}

#[tokio::test]
async fn revoking_one_path_leaves_another_alone() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "paths-owner@example.test").await;
    let abeer = world::person(&harness, "Abeer", "paths-reader@example.test").await;
    let group = world::group(&harness, &ronit, "Team", None)
        .await
        .expect("creates");
    let doc = world::document(&harness, &ronit, "Kernel report", None)
        .await
        .expect("creates");

    // Abeer reaches it directly and through the group.
    relation::grant(
        harness.store(),
        Object::document(doc),
        Relation::Viewer,
        relation::Subject::person(abeer.person()),
    )
    .await
    .expect("direct");
    relation::grant(
        harness.store(),
        Object::document(doc),
        Relation::Viewer,
        relation::Subject::group(group),
    )
    .await
    .expect("group");

    cmd::revoke(
        &harness.ctx(ronit.principal.clone()),
        &Revoke {
            resource: public(key(&harness), doc),
            subject: public(key(&harness), abeer.person()),
            relation: DocRole::Viewer,
        },
    )
    .await
    .expect("revokes");

    let left = relation::list_for_object(harness.store(), Object::document(doc))
        .await
        .expect("lists");
    assert_eq!(
        left.iter()
            .filter(|g| g.relation == Relation::Viewer)
            .count(),
        1,
        "the group's grant is somebody else's decision"
    );
}

#[tokio::test]
async fn transfer_is_the_only_way_an_owner_changes() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "xfer-from@example.test").await;
    let abeer = world::person(&harness, "Abeer", "xfer-to@example.test").await;
    let doc = world::document(&harness, &ronit, "Kernel report", None)
        .await
        .expect("creates");
    let doc_id = public(key(&harness), doc);

    // Not yours, not transferable.
    let refused = cmd::transfer(
        &harness.ctx(abeer.principal.clone()),
        &Transfer {
            resource: doc_id.clone(),
            to: public(key(&harness), abeer.person()),
        },
    )
    .await;
    assert!(refused.is_err_and(|err| err.is_decline()));

    let committed = cmd::transfer(
        &harness.ctx(ronit.principal.clone()),
        &Transfer {
            resource: doc_id,
            to: public(key(&harness), abeer.person()),
        },
    )
    .await
    .expect("the owner may");
    assert!(matches!(committed.event, Event::Transferred { .. }));

    let row = resource::load(harness.store(), doc)
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(row.owner_party_id, abeer.person());

    // The owner relation row moved with the column, so `check` agrees.
    let now_theirs = expand(harness.store(), &abeer.principal)
        .await
        .expect("expands");
    assert!(
        relation::check(
            harness.store(),
            &now_theirs,
            Relation::Editor,
            Object::document(doc)
        )
        .await
        .expect("checks")
    );
    let no_longer = expand(harness.store(), &ronit.principal)
        .await
        .expect("expands");
    assert!(
        !relation::check(
            harness.store(),
            &no_longer,
            Relation::Editor,
            Object::document(doc)
        )
        .await
        .expect("checks")
    );
}

#[tokio::test]
async fn a_group_is_refused_as_the_new_owner() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "xfer-group@example.test").await;
    let group = world::group(&harness, &ronit, "Team", None)
        .await
        .expect("creates");
    let doc = world::document(&harness, &ronit, "Kernel report", None)
        .await
        .expect("creates");

    let refused = cmd::transfer(
        &harness.ctx(ronit.principal.clone()),
        &Transfer {
            resource: public(key(&harness), doc),
            to: public(key(&harness), group),
        },
    )
    .await;
    assert!(
        refused.is_err_and(|err| err.is_decline()),
        "groups are granted, never owners"
    );
}

#[tokio::test]
async fn transferring_an_organization_moves_its_owner_membership_too() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "xfer-org@example.test").await;
    let abeer = world::person(&harness, "Abeer", "xfer-org-to@example.test").await;
    let (org, resource_id) = world::organization(&harness, &ronit, "Isoastra")
        .await
        .expect("founds");

    cmd::transfer(
        &harness.ctx(ronit.principal.clone()),
        &Transfer {
            resource: public(key(&harness), resource_id),
            to: public(key(&harness), abeer.person()),
        },
    )
    .await
    .expect("the owner may");

    let container = Id::new(org.get());
    assert_eq!(
        crate::org::role_of(harness.store(), container, abeer.person())
            .await
            .expect("reads"),
        Some(crate::org::MemberRole::Owner)
    );
    assert_eq!(
        crate::org::role_of(harness.store(), container, ronit.person())
            .await
            .expect("reads"),
        Some(crate::org::MemberRole::Admin),
        "handing an organization on is not leaving it"
    );
}

#[tokio::test]
async fn a_container_is_not_shared_by_its_resource_id() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "share-org@example.test").await;
    let abeer = world::person(&harness, "Abeer", "share-org-to@example.test").await;
    let (_, resource_id) = world::organization(&harness, &ronit, "Isoastra")
        .await
        .expect("founds");

    let refused = cmd::share(
        &harness.ctx(ronit.principal.clone()),
        &Share {
            resource: public(key(&harness), resource_id),
            subject: public(key(&harness), abeer.person()),
            relation: DocRole::Viewer,
        },
    )
    .await;
    assert!(
        refused.is_err_and(|err| err.is_decline()),
        "an organization is addressed by its party, and its members are a membership"
    );
}
