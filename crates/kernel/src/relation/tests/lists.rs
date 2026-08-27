//! `list_visible` and `visible_contacts` — the two listings, and the leak
//! neither of them is allowed to have.

use super::super::*;
use super::{db, member, party, resource, subjects};
use crate::ids::Id;
use crate::resource::kinds;

#[tokio::test]
async fn list_visible_joins_ownership_and_relations_and_pages_newest_first() {
    let store = db();
    let owner = party(&store, "person", "Owner").await;
    let reader = party(&store, "person", "Reader").await;
    let team = party(&store, "group", "Team").await;

    let mut mine = Vec::new();
    for n in 0..5 {
        let id = resource(&store, kinds::DOCUMENT, owner, 1_800_000_000 + n).await;
        mine.push(id);
        grant(
            &store,
            Object::document(Id::new(id)),
            Relation::Owner,
            Subject::person(Id::new(owner)),
        )
        .await
        .expect("owner row");
    }
    // One document the reader reaches through a group, and one they do not
    // reach at all.
    let shared = resource(&store, kinds::DOCUMENT, owner, 1_800_000_010).await;
    grant(
        &store,
        Object::document(Id::new(shared)),
        Relation::Viewer,
        Subject::group(Id::new(team)),
    )
    .await
    .expect("group row");
    let hidden = resource(&store, kinds::DOCUMENT, owner, 1_800_000_011).await;

    let owning = subjects(owner, &[], &[]);
    let page = list_visible(&store, &owning, kinds::DOCUMENT, Page::first(50))
        .await
        .expect("lists");
    assert_eq!(page.len(), 7, "everything the owner owns, shared or not");
    assert!(
        page.windows(2)
            .all(|pair| pair[0].created_at >= pair[1].created_at),
        "newest first"
    );

    let reading = subjects(reader, &[team], &[]);
    let theirs = list_visible(&store, &reading, kinds::DOCUMENT, Page::first(50))
        .await
        .expect("lists");
    assert_eq!(theirs.len(), 1);
    assert_eq!(theirs[0].id.get(), shared);
    assert!(theirs.iter().all(|row| row.id.get() != hidden));

    // Paging resumes where the previous page stopped and never repeats a row.
    let first = list_visible(&store, &owning, kinds::DOCUMENT, Page::first(3))
        .await
        .expect("lists");
    assert_eq!(first.len(), 3);
    let second = list_visible(&store, &owning, kinds::DOCUMENT, Page::after(&first[2], 3))
        .await
        .expect("lists");
    assert_eq!(second.len(), 3);
    for row in &second {
        assert!(first.iter().all(|seen| seen.id != row.id));
    }
}

#[tokio::test]
async fn a_deleted_resource_leaves_every_listing() {
    let store = db();
    let owner = party(&store, "person", "Owner").await;
    let doc = resource(&store, kinds::DOCUMENT, owner, 1_800_000_000).await;
    grant(
        &store,
        Object::document(Id::new(doc)),
        Relation::Owner,
        Subject::person(Id::new(owner)),
    )
    .await
    .expect("owner row");
    let owning = subjects(owner, &[], &[]);
    assert_eq!(
        list_visible(&store, &owning, kinds::DOCUMENT, Page::default())
            .await
            .expect("lists")
            .len(),
        1
    );

    store
        .execute(
            "UPDATE resource SET status = 'deleted' WHERE id = $1",
            bind![doc],
        )
        .await
        .expect("deletes");
    assert!(
        list_visible(&store, &owning, kinds::DOCUMENT, Page::default())
            .await
            .expect("lists")
            .is_empty()
    );
}

#[tokio::test]
async fn contact_details_are_visible_inside_a_group_and_nowhere_else() {
    let store = db();
    let ronit = party(&store, "person", "Ronit").await;
    let abeer = party(&store, "person", "Abeer").await;
    let outsider = party(&store, "person", "Outsider").await;
    let team = party(&store, "group", "Team").await;
    let other = party(&store, "group", "Other").await;
    member(&store, team, ronit, "member").await;
    member(&store, team, abeer, "member").await;
    member(&store, other, outsider, "member").await;

    // Abeer's details are scoped to the team. Nothing else is written.
    grant(
        &store,
        Object::person(Id::new(abeer)),
        Relation::Contact,
        Subject::group(Id::new(team)),
    )
    .await
    .expect("contact row");
    // The outsider's details are scoped to their own group.
    grant(
        &store,
        Object::person(Id::new(outsider)),
        Relation::Contact,
        Subject::group(Id::new(other)),
    )
    .await
    .expect("contact row");

    let inside = subjects(ronit, &[team], &[]);
    let seen = visible_contacts(&store, &inside, Id::new(team))
        .await
        .expect("lists");
    assert_eq!(seen, vec![Id::new(abeer)]);

    // The other group's contact is not reachable from here, and the group the
    // caller does not belong to answers with nobody at all.
    assert!(
        !visible_contacts(&store, &inside, Id::new(team))
            .await
            .expect("lists")
            .contains(&Id::new(outsider))
    );
    assert!(
        visible_contacts(&store, &inside, Id::new(other))
            .await
            .expect("lists")
            .is_empty(),
        "a group the principal is not in shows nobody"
    );
}
