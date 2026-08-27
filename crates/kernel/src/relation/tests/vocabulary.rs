//! The vocabulary, in both halves: the words the schema admits, and the
//! nesting the code applies to them.

use super::super::*;
use super::db;
use crate::domain::Vocabulary as _;
use crate::store::{Cursor, FromRow, RowError, Sqlite, Store, StoreError};

/// The trigger's `IN (…)` list, read back out of the schema.
async fn admitted_relations(store: &Store<Sqlite>) -> Vec<String> {
    struct Ddl(String);
    impl FromRow for Ddl {
        fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
            Ok(Self(row.text("sql")?))
        }
    }
    let ddl = store
        .query_one::<Ddl>(
            "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = $1",
            bind!["relation_vocabulary_insert"],
        )
        .await
        .expect("the trigger exists")
        .0;
    let open = ddl.find("IN (").expect("an IN list") + 4;
    let close = ddl[open..].find(')').expect("a closed IN list") + open;
    ddl[open..close]
        .split(',')
        .map(|v| v.trim().trim_matches('\'').to_owned())
        .collect()
}

#[tokio::test]
async fn the_relation_vocabulary_agrees_with_the_schema() {
    let store = db();
    let schema = admitted_relations(&store).await;
    let code: Vec<String> = Relation::ALL
        .iter()
        .map(|r| r.as_str().to_owned())
        .collect();
    assert_eq!(schema, code, "the trigger and the enum disagree");
}

#[tokio::test]
async fn subject_kind_agrees_with_the_schema() {
    let store = db();
    let kinds: Vec<String> = <SubjectKind as crate::domain::Vocabulary>::ALL
        .iter()
        .map(|k| k.as_str().to_owned())
        .collect();
    for kind in &kinds {
        let refused = store
            .execute(
                "INSERT INTO relation (object_kind, object_id, relation, subject_kind, \
                 subject_id, at) VALUES ('document', 1, 'viewer', $1, $2, 1)",
                bind![
                    kind.as_str(),
                    i64::from(!kind.starts_with("pub") && kind != "authenticated")
                ],
            )
            .await;
        assert!(refused.is_ok(), "the schema refuses {kind}: {refused:?}");
    }
    let unknown = store
        .execute(
            "INSERT INTO relation (object_kind, object_id, relation, subject_kind, subject_id, at) \
             VALUES ('document', 2, 'viewer', 'wizard', 1, 1)",
            bind![],
        )
        .await;
    assert!(matches!(unknown, Err(StoreError::Constraint(_))));
}

#[tokio::test]
async fn a_relation_this_deployment_does_not_know_is_refused_by_the_database() {
    let store = db();
    let refused = store
        .execute(
            "INSERT INTO relation (object_kind, object_id, relation, subject_kind, subject_id, at) \
             VALUES ('document', 1, 'superuser', 'person', 1, 1)",
            bind![],
        )
        .await;
    assert!(
        matches!(
            refused,
            Err(StoreError::Backend(_) | StoreError::Constraint(_))
        ),
        "the trigger must refuse it: {refused:?}"
    );
}

#[test]
fn nesting_is_a_partial_order() {
    for a in Relation::ALL {
        assert!(a.covers(*a), "{a:?} must cover itself");
        for b in Relation::ALL {
            if a.covers(*b) && b.covers(*a) {
                assert_eq!(a, b, "{a:?} and {b:?} cover each other without being equal");
            }
            for c in Relation::ALL {
                if a.covers(*b) && b.covers(*c) {
                    assert!(a.covers(*c), "{a:?} ⊇ {b:?} ⊇ {c:?} but not {a:?} ⊇ {c:?}");
                }
            }
        }
    }
}

#[test]
fn owner_is_the_top_of_both_ladders_and_nothing_reaches_across() {
    for relation in [
        Relation::Viewer,
        Relation::Commenter,
        Relation::Editor,
        Relation::Member,
        Relation::Admin,
    ] {
        assert!(Relation::Owner.covers(relation), "owner ⊉ {relation:?}");
        assert!(!relation.covers(Relation::Owner));
    }
    assert!(Relation::Editor.covers(Relation::Viewer));
    assert!(!Relation::Editor.covers(Relation::Member));
    assert!(!Relation::Admin.covers(Relation::Editor));
    assert!(!Relation::Contact.covers(Relation::Viewer));
    assert!(!Relation::Operator.covers(Relation::Owner));
}

/// The kernel report's example: a product kind that accepts a person and an
/// organization and refuses `@public` outright.
const PATIENT: Vocabulary = Vocabulary::new(&[Kind {
    name: "patient",
    is_resource: true,
    is_container: false,
    admits: &[Admits {
        relation: Relation::Viewer,
        subjects: PARTY_SUBJECTS,
    }],
}]);

#[test]
fn a_kind_that_refuses_public_cannot_be_granted_it() {
    assert!(PATIENT.admits("patient", Relation::Viewer, SubjectKind::Person));
    assert!(PATIENT.admits("patient", Relation::Viewer, SubjectKind::Group));
    assert!(!PATIENT.admits("patient", Relation::Viewer, SubjectKind::Public));
    assert!(!PATIENT.admits("patient", Relation::Viewer, SubjectKind::Authenticated));
    // And nothing else about that kind is admitted either.
    assert!(!PATIENT.admits("patient", Relation::Editor, SubjectKind::Person));
    // An unregistered kind admits nothing, so a typo writes no rows.
    assert!(!PATIENT.admits("patinet", Relation::Viewer, SubjectKind::Person));
    // A document, by contrast, is a kind that does accept the world.
    assert!(Vocabulary::KERNEL.admits("document", Relation::Viewer, SubjectKind::Public));
}
