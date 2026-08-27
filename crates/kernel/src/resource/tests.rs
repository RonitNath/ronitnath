//! The registry's two schema-carried rules, and the status vocabulary.

use super::*;
use crate::domain::Vocabulary as _;
use crate::store::{Sqlite, Store, StoreError};

fn db() -> Store<Sqlite> {
    crate::store::tests::store()
}

async fn party(store: &Store<Sqlite>, kind: &'static str) -> i64 {
    store
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) \
             VALUES ($1, 'A party', 'active', 1)",
            bind![kind],
        )
        .await
        .expect("the party lands");
    store
        .query::<crate::store::Count>("SELECT max(id) AS n FROM party", bind![])
        .await
        .expect("reads")[0]
        .0
}

#[tokio::test]
async fn the_status_vocabulary_agrees_with_the_schema() {
    let store = db();
    let owner = party(&store, "person").await;
    for status in ResourceStatus::ALL {
        store
            .query::<crate::store::RowId<Resource>>(
                INSERT_SQL,
                bind![kinds::DOCUMENT, owner, "us-west", status.as_str(), 1i64],
            )
            .await
            .unwrap_or_else(|err| panic!("the schema refuses {}: {err:?}", status.as_str()));
    }
    let refused = store
        .query::<crate::store::RowId<Resource>>(
            INSERT_SQL,
            bind![kinds::DOCUMENT, owner, "us-west", "archived", 1i64],
        )
        .await;
    assert!(
        matches!(refused, Err(StoreError::Constraint(_))),
        "a CHECK constraint, not a convention: {refused:?}"
    );
}

#[tokio::test]
async fn a_group_can_never_be_an_owner_however_the_row_arrives() {
    let store = db();
    let person = party(&store, "person").await;
    let group = party(&store, "group").await;

    let refused = store
        .query::<crate::store::RowId<Resource>>(
            INSERT_SQL,
            bind![kinds::DOCUMENT, group, "us-west", "draft", 1i64],
        )
        .await;
    assert!(refused.is_err(), "a group must not own on insert");

    let id = store
        .query::<crate::store::RowId<Resource>>(
            INSERT_SQL,
            bind![kinds::DOCUMENT, person, "us-west", "draft", 1i64],
        )
        .await
        .expect("a person may own")[0]
        .0
        .get();
    let moved = store.execute(TRANSFER_SQL, bind![group, id, person]).await;
    assert!(moved.is_err(), "a group must not own on update either");
}

#[tokio::test]
async fn a_party_backed_resource_is_read_back_from_either_end() {
    let store = db();
    let founder = party(&store, "person").await;
    let org = party(&store, "organization").await;
    let resource_id = store
        .query::<crate::store::RowId<Resource>>(
            INSERT_SQL,
            bind![kinds::ORGANIZATION, founder, "us-west", "published", 1i64],
        )
        .await
        .expect("the resource lands")[0]
        .0
        .get();
    store
        .execute(INSERT_PARTY_RESOURCE_SQL, bind![resource_id, org])
        .await
        .expect("the join lands");

    let by_id = load(&store, Id::new(resource_id))
        .await
        .expect("reads")
        .expect("it is there");
    let by_party = of_party(&store, Id::new(org))
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(by_id, by_party);
    assert_eq!(by_id.owner_party_id.get(), founder);
    assert_eq!(by_id.status, ResourceStatus::Published);
}
