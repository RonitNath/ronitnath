//! The relation store: the vocabulary, the nesting, and the three queries.

mod lists;
mod vocabulary;

use super::*;
use crate::ids::Id;
use crate::store::{Count, Sqlite, Store};

pub(super) fn db() -> Store<Sqlite> {
    crate::store::tests::store()
}

async fn insert(store: &Store<Sqlite>, sql: &'static str, params: Vec<crate::store::Value>) {
    store.execute(sql, params).await.expect("the insert lands");
}

async fn last_id(store: &Store<Sqlite>, table: &'static str) -> i64 {
    let sql = match table {
        "party" => "SELECT max(id) AS n FROM party",
        _ => "SELECT max(id) AS n FROM resource",
    };
    store.query::<Count>(sql, bind![]).await.expect("reads")[0].0
}

/// A party of some kind, returning its id.
pub(super) async fn party(store: &Store<Sqlite>, kind: &'static str, name: &'static str) -> i64 {
    insert(
        store,
        "INSERT INTO party (kind, display_name, status, created_at) VALUES ($1, $2, 'active', $3)",
        bind![kind, name, 1_800_000_000i64],
    )
    .await;
    last_id(store, "party").await
}

/// A resource of some kind, returning its id. `INSERT … RETURNING` is a query
/// rather than an execute, which is what the engine's error message is telling
/// a caller that gets this wrong.
pub(super) async fn resource(
    store: &Store<Sqlite>,
    kind: &'static str,
    owner: i64,
    at: i64,
) -> i64 {
    store
        .query::<crate::store::RowId<crate::ids::Resource>>(
            crate::resource::INSERT_SQL,
            bind![kind, owner, "us-west", "published", at],
        )
        .await
        .expect("the resource lands")[0]
        .0
        .get()
}

pub(super) async fn member(store: &Store<Sqlite>, container: i64, who: i64, role: &'static str) {
    insert(
        store,
        crate::org::INSERT_SQL,
        bind![container, who, role, 1_800_000_000i64],
    )
    .await;
}

pub(super) fn subjects(person: i64, groups: &[i64], organizations: &[i64]) -> SubjectSet {
    SubjectSet {
        identity: Some(Id::new(1)),
        person: Some(Id::new(person)),
        organizations: organizations.iter().map(|o| Id::new(*o)).collect(),
        groups: groups.iter().map(|g| Id::new(*g)).collect(),
        link: None,
        authenticated: true,
    }
}

// ---------------------------------------------------------------- grants ---

#[tokio::test]
async fn a_grant_is_written_once_and_listed_from_both_ends() {
    let store = db();
    let ronit = party(&store, "person", "Ronit").await;
    let doc = resource(&store, "document", ronit, 1_800_000_000).await;
    let object = Object::document(Id::new(doc));
    let subject = Subject::person(Id::new(ronit));

    grant(&store, object, Relation::Editor, subject)
        .await
        .expect("grants");
    grant(&store, object, Relation::Editor, subject)
        .await
        .expect("granting twice is the same world");

    let on_object = list_for_object(&store, object).await.expect("lists");
    assert_eq!(on_object.len(), 1);
    assert_eq!(on_object[0].relation, Relation::Editor);
    assert_eq!(on_object[0].subject, subject);
    assert_eq!(
        list_for_subject(&store, subject)
            .await
            .expect("lists")
            .len(),
        1
    );

    revoke(&store, object, Relation::Editor, subject)
        .await
        .expect("revokes");
    assert!(
        list_for_object(&store, object)
            .await
            .expect("lists")
            .is_empty()
    );
}

// ----------------------------------------------------------------- check ---

#[tokio::test]
async fn check_reads_ownership_a_direct_grant_and_a_group_grant() {
    let store = db();
    let owner = party(&store, "person", "Owner").await;
    let reader = party(&store, "person", "Reader").await;
    let stranger = party(&store, "person", "Stranger").await;
    let team = party(&store, "group", "Team").await;
    let doc = resource(&store, "document", owner, 1_800_000_000).await;
    let object = Object::document(Id::new(doc));

    grant(
        &store,
        object,
        Relation::Owner,
        Subject::person(Id::new(owner)),
    )
    .await
    .expect("owner row");
    grant(
        &store,
        object,
        Relation::Viewer,
        Subject::group(Id::new(team)),
    )
    .await
    .expect("group row");

    // The owner may do anything to their own thing.
    let owning = subjects(owner, &[], &[]);
    assert!(
        check(&store, &owning, Relation::Editor, object)
            .await
            .unwrap()
    );
    assert!(
        check(&store, &owning, Relation::Viewer, object)
            .await
            .unwrap()
    );

    // The group's viewer may read and no more.
    let in_group = subjects(reader, &[team], &[]);
    assert!(
        check(&store, &in_group, Relation::Viewer, object)
            .await
            .unwrap()
    );
    assert!(
        !check(&store, &in_group, Relation::Editor, object)
            .await
            .unwrap()
    );

    // Everyone else, nothing.
    let outside = subjects(stranger, &[], &[]);
    assert!(
        !check(&store, &outside, Relation::Viewer, object)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn check_reads_membership_for_a_container() {
    let store = db();
    let boss = party(&store, "person", "Boss").await;
    let hand = party(&store, "person", "Hand").await;
    let org = party(&store, "organization", "Isoastra").await;
    member(&store, org, boss, "owner").await;
    member(&store, org, hand, "member").await;
    let object = Object::organization(Id::new(org));

    assert!(
        check(
            &store,
            &subjects(boss, &[], &[org]),
            Relation::Admin,
            object
        )
        .await
        .unwrap()
    );
    assert!(
        !check(
            &store,
            &subjects(hand, &[], &[org]),
            Relation::Admin,
            object
        )
        .await
        .unwrap()
    );
    assert!(
        check(
            &store,
            &subjects(hand, &[], &[org]),
            Relation::Member,
            object
        )
        .await
        .unwrap()
    );
}

#[tokio::test]
async fn public_reaches_an_anonymous_principal_and_authenticated_does_not() {
    let store = db();
    let owner = party(&store, "person", "Owner").await;
    let doc = resource(&store, "document", owner, 1_800_000_000).await;
    let object = Object::document(Id::new(doc));
    grant(&store, object, Relation::Viewer, Subject::public())
        .await
        .expect("public row");

    let nobody = SubjectSet::default();
    assert!(
        check(&store, &nobody, Relation::Viewer, object)
            .await
            .unwrap()
    );

    let other = resource(&store, "document", owner, 1_800_000_001).await;
    let other_object = Object::document(Id::new(other));
    grant(
        &store,
        other_object,
        Relation::Viewer,
        Subject::authenticated(),
    )
    .await
    .expect("authenticated row");
    assert!(
        !check(&store, &nobody, Relation::Viewer, other_object)
            .await
            .unwrap()
    );
    assert!(
        check(
            &store,
            &subjects(owner, &[], &[]),
            Relation::Viewer,
            other_object
        )
        .await
        .unwrap()
    );
}
