//! Roles, and the membership questions every container command asks.

use super::*;
use crate::store::{Sqlite, Store, StoreError};

fn db() -> Store<Sqlite> {
    crate::store::tests::store()
}

async fn party(store: &Store<Sqlite>, kind: &'static str, name: &'static str) -> Id<Person> {
    store
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) \
             VALUES ($1, $2, 'active', 1)",
            bind![kind, name],
        )
        .await
        .expect("the party lands");
    Id::new(
        store
            .query::<Count>("SELECT max(id) AS n FROM party", bind![])
            .await
            .expect("reads")[0]
            .0,
    )
}

async fn join(store: &Store<Sqlite>, container: Id<Person>, who: Id<Person>, role: MemberRole) {
    store
        .execute(INSERT_SQL, bind![container, who, role.as_str(), 1i64])
        .await
        .expect("the membership lands");
}

#[tokio::test]
async fn the_role_vocabulary_agrees_with_the_schema() {
    let store = db();
    let org = party(&store, "organization", "Isoastra").await;
    for role in MemberRole::ALL {
        let who = party(&store, "person", "Somebody").await;
        store
            .execute(INSERT_SQL, bind![org, who, role.as_str(), 1i64])
            .await
            .unwrap_or_else(|err| panic!("the schema refuses {}: {err:?}", role.as_str()));
    }
    let who = party(&store, "person", "Somebody").await;
    let refused = store
        .execute(INSERT_SQL, bind![org, who, "superuser", 1i64])
        .await;
    assert!(matches!(refused, Err(StoreError::Constraint(_))));
}

#[test]
fn roles_nest_the_way_the_report_says() {
    assert!(MemberRole::Owner.covers(MemberRole::Admin));
    assert!(MemberRole::Owner.covers(MemberRole::Member));
    assert!(MemberRole::Admin.covers(MemberRole::Member));
    assert!(!MemberRole::Member.covers(MemberRole::Admin));
    assert!(!MemberRole::Admin.covers(MemberRole::Owner));
    for role in MemberRole::ALL {
        assert!(role.covers(*role));
    }
}

#[tokio::test]
async fn a_container_answers_who_belongs_and_how_many_own_it() {
    let store = db();
    let org = party(&store, "organization", "Isoastra").await;
    let boss = party(&store, "person", "Ronit").await;
    let hand = party(&store, "person", "Abeer").await;
    let stranger = party(&store, "person", "Nobody").await;
    join(&store, org, boss, MemberRole::Owner).await;
    join(&store, org, hand, MemberRole::Member).await;

    assert_eq!(
        role_of(&store, org, boss).await.unwrap(),
        Some(MemberRole::Owner)
    );
    assert_eq!(role_of(&store, org, stranger).await.unwrap(), None);
    assert!(holds(&store, org, boss, MemberRole::Admin).await.unwrap());
    assert!(!holds(&store, org, hand, MemberRole::Admin).await.unwrap());
    assert!(
        !holds(&store, org, stranger, MemberRole::Member)
            .await
            .unwrap()
    );
    assert_eq!(owner_count(&store, org).await.unwrap(), 1);

    let listed = members(&store, org).await.expect("lists");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].party.display_name, "Ronit");
    assert_eq!(listed[0].membership.role, MemberRole::Owner);
    assert_eq!(listed[1].party.display_name, "Abeer");
}

#[tokio::test]
async fn one_party_holds_one_role_in_one_container() {
    let store = db();
    let org = party(&store, "organization", "Isoastra").await;
    let who = party(&store, "person", "Ronit").await;
    join(&store, org, who, MemberRole::Member).await;
    let again = store
        .execute(INSERT_SQL, bind![org, who, "admin", 2i64])
        .await;
    assert!(
        matches!(again, Err(StoreError::Constraint(_))),
        "the primary key is what makes a role single-valued: {again:?}"
    );
}
