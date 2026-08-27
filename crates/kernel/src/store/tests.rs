//! Storage tests: the schema the migration produces, what a batch means, and
//! the compile-time proof that a read handle cannot write.

use super::*;
use crate::bind;

/// A key that says what it is.
const TEST_KEY: &str = "0f0e0d0c0b0a09080706050403020100";

pub(crate) fn store() -> Store<Sqlite> {
    Store::new(
        Sqlite::memory().expect("in-memory database opens"),
        IdKey::from_hex(TEST_KEY).expect("test key is well formed"),
        Clock::fixed(1_800_000_000),
    )
}

// ------------------------------------------------------------- read-only ---

/// `ReadStore` having no way to write is proved by the two `compile_fail`
/// doctests on the type itself: a compile error is the only assertion that an
/// *absence* accepts, since stable Rust has no negative bound. What is checked
/// here is the other half — that the split is real and a `Store` does write.
#[test]
fn a_store_writes_and_a_read_store_is_a_different_type() {
    fn writes<T: Writes>() {}
    fn reads<T: Reads>() {}
    writes::<Store<Sqlite>>();
    reads::<Store<Sqlite>>();
    reads::<ReadStore<'static, Sqlite>>();
}

// ---------------------------------------------------------------- schema ---

#[derive(Debug)]
struct Name(String);

impl FromRow for Name {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.text("name")?))
    }
}

async fn names(store: &Store<Sqlite>, kind: &'static str) -> Vec<String> {
    store
        .query::<Name>(
            "SELECT name FROM sqlite_master WHERE type = $1 AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
            bind![kind],
        )
        .await
        .expect("sqlite_master is readable")
        .into_iter()
        .map(|n| n.0)
        .collect()
}

#[tokio::test]
async fn the_migration_creates_exactly_the_reports_tables() {
    let store = store();
    assert_eq!(
        names(&store, "table").await,
        vec![
            "audit",
            // K2's migration adds `document` and `party_resource`; the list is
            // exhaustive on purpose, so a table nobody meant to add fails here.
            "document",
            "factor",
            "identity",
            "link",
            "match_candidate",
            "membership",
            "party",
            "party_resource",
            "person_alias",
            "person_link",
            "relation",
            "resource",
            "session",
        ]
    );
}

#[tokio::test]
async fn every_named_index_exists() {
    let store = store();
    let indexes = names(&store, "index").await;
    for wanted in [
        "audit_actor_idx",
        "factor_email_unique_idx",
        "factor_identity_idx",
        "factor_verified_value_idx",
        "identity_person_idx",
        "identity_source_idx",
        "match_candidate_a_idx",
        "match_candidate_b_idx",
        "match_candidate_pair_idx",
        "match_candidate_queue_idx",
        "membership_party_idx",
        "party_kind_status_idx",
        "person_alias_person_idx",
        "person_link_identity_idx",
        "person_link_person_idx",
        "relation_object_idx",
        "relation_subject_idx",
        "relation_subject_key_idx",
        "relation_unique_idx",
        "resource_kind_created_idx",
        "resource_owner_idx",
        "session_expires_idx",
        "session_identity_idx",
    ] {
        assert!(indexes.contains(&wanted.to_owned()), "missing {wanted}");
    }
}

#[tokio::test]
async fn a_status_no_command_produces_is_refused_by_the_schema() {
    let store = store();
    let bad = store
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) VALUES ($1, $2, $3, $4)",
            bind!["person", "Nobody", "pending", 1i64],
        )
        .await;
    assert!(
        matches!(bad, Err(StoreError::Constraint(_))),
        "a CHECK constraint, not a convention: {bad:?}"
    );
}

#[tokio::test]
async fn one_address_registers_once() {
    let store = store();
    seed_identity(&store, "ronit@example.test").await;
    let again = store
        .execute(
            "INSERT INTO factor (identity_id, kind, value, created_at) VALUES ($1, $2, $3, $4)",
            bind![1i64, "email", "ronit@example.test", 1i64],
        )
        .await;
    assert!(matches!(again, Err(StoreError::Constraint(_))));

    // The uniqueness is partial: two password factors share nothing.
    store
        .execute(
            "INSERT INTO factor (identity_id, kind, value, created_at) VALUES ($1, $2, $3, $4)",
            bind![1i64, "password", "$argon2id$fake", 1i64],
        )
        .await
        .expect("a second password factor is not a duplicate address");
}

#[tokio::test]
async fn a_group_never_owns_a_resource() {
    let store = store();
    store
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) VALUES ($1, $2, $3, $4)",
            bind!["group", "Cohort", "active", 1i64],
        )
        .await
        .expect("groups exist");
    let refused = store
        .execute(
            "INSERT INTO resource (kind, owner_party_id, home_zone, status, created_at) \
             VALUES ($1, $2, $3, $4, $5)",
            bind!["document", 1i64, "us-west", "draft", 1i64],
        )
        .await;
    assert!(refused.is_err(), "the trigger refuses it: {refused:?}");
}

// ----------------------------------------------------------------- batch ---

async fn seed_identity(store: &Store<Sqlite>, email: &str) {
    store
        .commit(vec![
            Stmt::one(
                "INSERT INTO party (kind, display_name, status, created_at) \
                 VALUES ('person', $1, 'active', $2) RETURNING id",
                bind![email, 1i64],
            ),
            Stmt::one(
                "INSERT INTO identity (source, person_id, home_zone, status, created_at) \
                 VALUES ('rn-site', $1, 'us-west', 'active', $2) RETURNING id",
                vec![Value::of_stmt(0, "id"), Value::Int(1)],
            ),
            Stmt::one(
                "INSERT INTO factor (identity_id, kind, value, created_at) \
                 VALUES ($1, 'email', $2, $3)",
                vec![Value::of_stmt(1, "id"), Value::from(email), Value::Int(1)],
            ),
        ])
        .await
        .expect("the batch commits");
}

#[tokio::test]
async fn a_statement_can_bind_an_earlier_statements_returned_id() {
    let store = store();
    seed_identity(&store, "linked@example.test").await;
    let rows: Vec<Count> = store
        .query(
            "SELECT count(*) AS n FROM factor \
             JOIN identity ON identity.id = factor.identity_id \
             JOIN party ON party.id = identity.person_id \
             WHERE party.display_name = $1",
            bind!["linked@example.test"],
        )
        .await
        .expect("query runs");
    assert_eq!(rows[0].0, 1, "the batch wired all three rows together");
}

#[tokio::test]
async fn a_guard_that_misses_writes_nothing_and_says_so() {
    let store = store();
    seed_identity(&store, "guarded@example.test").await;

    // Both statements carry the same guard, so a miss is uniform — and the
    // audit row goes first, because every statement after it reads a world it
    // has already changed.
    let guarded = |status: &'static str, key: &'static str| {
        vec![
            Stmt::one(
                "INSERT INTO audit (key, command, at, request_digest, payload) \
                 SELECT $1, 'disable', 1, 'digest', '{}' \
                 WHERE EXISTS (SELECT 1 FROM party WHERE id = 1 AND status = $2)",
                bind![key, status],
            ),
            Stmt::one(
                "UPDATE party SET status = 'disabled' WHERE id = 1 AND status = $1",
                bind![status],
            ),
        ]
    };

    // The world has moved: nothing matches 'merged'.
    assert_eq!(
        store
            .commit(guarded("merged", "11111111-1111-1111-1111-111111111111"))
            .await
            .unwrap_err(),
        StoreError::GuardMissed
    );
    let audit: Vec<Count> = store
        .query("SELECT count(*) AS n FROM audit", bind![])
        .await
        .expect("query runs");
    assert_eq!(audit[0].0, 0, "a missed guard leaves no audit row behind");

    // The world is as it was read: the batch applies.
    store
        .commit(guarded("active", "22222222-2222-2222-2222-222222222222"))
        .await
        .expect("commits");
}

#[tokio::test]
async fn a_statement_missing_its_guard_is_reported_as_a_bug_not_a_decline() {
    let store = store();
    seed_identity(&store, "mixed@example.test").await;
    let outcome = store
        .commit(vec![
            Stmt::one("UPDATE party SET display_name = 'x' WHERE id = 1", bind![]),
            Stmt::one(
                "UPDATE party SET display_name = 'y' WHERE id = 999",
                bind![],
            ),
        ])
        .await;
    assert!(
        matches!(
            outcome,
            Err(StoreError::BatchMismatch {
                index: 1,
                rows: 0,
                ..
            })
        ),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_batch_that_errors_leaves_nothing_behind() {
    let store = store();
    let outcome = store
        .commit(vec![
            Stmt::one(
                "INSERT INTO party (kind, display_name, status, created_at) \
                 VALUES ('person', 'first', 'active', 1)",
                bind![],
            ),
            Stmt::one(
                "INSERT INTO party (kind, display_name, status, created_at) \
                 VALUES ('person', 'second', 'nonsense', 1)",
                bind![],
            ),
        ])
        .await;
    assert!(
        matches!(outcome, Err(StoreError::Constraint(_))),
        "{outcome:?}"
    );
    let parties: Vec<Count> = store
        .query("SELECT count(*) AS n FROM party", bind![])
        .await
        .expect("query runs");
    assert_eq!(parties[0].0, 0, "the transaction rolled back");
}

#[test]
fn a_clock_a_test_owns_moves_only_when_told() {
    let clock = Clock::fixed(1_000);
    assert_eq!(clock.now(), 1_000);
    clock.advance(60);
    assert_eq!(clock.now(), 1_060);
    assert!(
        Clock::System.now() > 1_700_000_000,
        "the system clock is real"
    );
}

#[test]
fn the_embedded_migrations_are_the_files_on_disk() {
    let sql = migrations();
    // Four files: `1_kernel.sql` creates every table, `2_relations.sql` adds
    // what the relation store needs on top of them, `3_merge.sql` what merge
    // needs, `4_invitations.sql` the index "invitations I minted" seeks on.
    // Each is asserted below by something only that file contains.
    assert_eq!(sql.len(), 4);
    assert!(sql[0].contains("CREATE TABLE party"));
    assert!(sql[2].contains("person_link_is_append_only_update"));
    assert!(sql[3].contains("relation_granted_by_idx"));
    // Order is what makes the second file able to alter the first's tables.
    assert!(sql[1].contains("ALTER TABLE relation"));
}
