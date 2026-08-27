//! Documents: the version tag, and the state transition.

use rn_api::commands::{CreateDocument, EditDocument, PublishDocument};

use super::world::{self, key, public};
use crate::cmd;
use crate::document;
use crate::error::KernelError;
use crate::event::Event;
use crate::principal::expand;
use crate::relation::{self, Object, Page, Relation};
use crate::resource::{self, ResourceStatus, kinds};
use crate::testing::Local;

#[tokio::test]
async fn a_new_document_is_a_draft_its_owner_can_reach() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "doc@example.test").await;
    let doc = world::document(&harness, &ronit, "Kernel report", None)
        .await
        .expect("creates");

    let row = document::load(harness.store(), doc)
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(row.title, "Kernel report");
    assert_eq!(row.draft_rev, document::FIRST_REV);
    assert_eq!(row.published_rev, None);
    assert!(row.has_unpublished_changes());

    let registered = resource::load(harness.store(), doc)
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(registered.status, ResourceStatus::Draft);
    assert_eq!(registered.owner_party_id, ronit.person());

    let subjects = expand(harness.store(), &ronit.principal)
        .await
        .expect("expands");
    assert!(
        relation::check(
            harness.store(),
            &subjects,
            Relation::Editor,
            Object::document(doc)
        )
        .await
        .expect("checks")
    );
    let listed =
        relation::list_visible(harness.store(), &subjects, kinds::DOCUMENT, Page::default())
            .await
            .expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, doc);
}

#[tokio::test]
async fn an_edit_against_a_stale_revision_is_declined() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "edit@example.test").await;
    let doc = world::document(&harness, &ronit, "Kernel report", None)
        .await
        .expect("creates");
    let id = public(key(&harness), doc);

    let committed = cmd::edit_document(
        &harness.ctx(ronit.principal.clone()),
        &EditDocument {
            document: id.clone(),
            expected_rev: 1,
            title: Some("Kernel report, revised".to_owned()),
            body: None,
        },
    )
    .await
    .expect("edits");
    assert_eq!(
        committed.event,
        Event::DocumentEdited {
            document: doc,
            rev: 2
        }
    );
    let row = document::load(harness.store(), doc)
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(row.title, "Kernel report, revised");
    assert_eq!(row.body, "Five rules", "a title change keeps the body");
    assert_eq!(row.draft_rev, 2);

    // The other tab, still holding revision 1.
    let stale = cmd::edit_document(
        &harness.ctx(ronit.principal.clone()),
        &EditDocument {
            document: id,
            expected_rev: 1,
            title: Some("Kernel report, mine".to_owned()),
            body: None,
        },
    )
    .await;
    assert!(stale.is_err_and(|err| err.is_decline()));
    assert_eq!(
        document::load(harness.store(), doc)
            .await
            .expect("reads")
            .expect("it is there")
            .title,
        "Kernel report, revised"
    );
}

#[tokio::test]
async fn an_edit_with_nothing_in_it_is_refused_rather_than_bumping_the_revision() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "edit-empty@example.test").await;
    let doc = world::document(&harness, &ronit, "Kernel report", None)
        .await
        .expect("creates");
    let empty = cmd::edit_document(
        &harness.ctx(ronit.principal.clone()),
        &EditDocument {
            document: public(key(&harness), doc),
            expected_rev: 1,
            title: None,
            body: None,
        },
    )
    .await;
    assert!(matches!(empty, Err(KernelError::Invalid(_))));
}

#[tokio::test]
async fn a_stranger_cannot_edit_and_a_shared_editor_can() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "edit-owner@example.test").await;
    let abeer = world::person(&harness, "Abeer", "edit-other@example.test").await;
    let doc = world::document(&harness, &ronit, "Kernel report", None)
        .await
        .expect("creates");
    let args = EditDocument {
        document: public(key(&harness), doc),
        expected_rev: 1,
        title: None,
        body: Some("Six rules".to_owned()),
    };

    let refused = cmd::edit_document(&harness.ctx(abeer.principal.clone()), &args).await;
    assert!(refused.is_err_and(|err| err.is_decline()));

    relation::grant(
        harness.store(),
        Object::document(doc),
        Relation::Editor,
        relation::Subject::person(abeer.person()),
    )
    .await
    .expect("shares");
    cmd::edit_document(&harness.ctx(abeer.principal.clone()), &args)
        .await
        .expect("an editor edits");
}

#[tokio::test]
async fn publishing_moves_the_status_once_and_the_revision_every_time() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "publish@example.test").await;
    let doc = world::document(&harness, &ronit, "Kernel report", None)
        .await
        .expect("creates");
    let id = public(key(&harness), doc);

    cmd::publish_document(
        &harness.ctx(ronit.principal.clone()),
        &PublishDocument {
            document: id.clone(),
        },
    )
    .await
    .expect("publishes");
    let row = document::load(harness.store(), doc)
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(row.published_rev, Some(1));
    assert!(!row.has_unpublished_changes());
    assert_eq!(
        resource::load(harness.store(), doc)
            .await
            .expect("reads")
            .expect("it is there")
            .status,
        ResourceStatus::Published
    );

    cmd::edit_document(
        &harness.ctx(ronit.principal.clone()),
        &EditDocument {
            document: id.clone(),
            expected_rev: 1,
            title: None,
            body: Some("Six rules".to_owned()),
        },
    )
    .await
    .expect("edits");
    assert!(
        document::load(harness.store(), doc)
            .await
            .expect("reads")
            .expect("it is there")
            .has_unpublished_changes()
    );

    cmd::publish_document(
        &harness.ctx(ronit.principal.clone()),
        &PublishDocument { document: id },
    )
    .await
    .expect("publishes again");
    assert_eq!(
        document::load(harness.store(), doc)
            .await
            .expect("reads")
            .expect("it is there")
            .published_rev,
        Some(2)
    );
}

#[tokio::test]
async fn a_document_can_be_owned_by_an_organization_a_member_belongs_to() {
    let harness = Local::new();
    let ronit = world::person(&harness, "Ronit", "org-doc@example.test").await;
    let abeer = world::person(&harness, "Abeer", "org-doc-other@example.test").await;
    let (org, _) = world::organization(&harness, &ronit, "Isoastra")
        .await
        .expect("founds");

    let outsider = cmd::create_document(
        &harness.ctx(abeer.principal.clone()),
        &CreateDocument {
            title: "Not mine".to_owned(),
            body: String::new(),
            owner: Some(public(key(&harness), org)),
        },
    )
    .await;
    assert!(outsider.is_err_and(|err| err.is_decline()));

    let doc = world::document(&harness, &ronit, "Kernel report", Some(org))
        .await
        .expect("creates");
    let row = resource::load(harness.store(), doc)
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(row.owner_party_id.get(), org.get());
}
