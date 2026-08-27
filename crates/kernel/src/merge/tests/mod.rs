//! Merge, against the real schema.
//!
//! Split by what is being proved rather than by which file the code is in: a
//! merge is a story about seven tables, and the tests that tell it belong
//! beside each other.

mod operator;
mod ownership;
mod proofs;
mod signals;
mod steps;

/// What every file here needs.
mod prelude {
    pub(super) use rn_api::commands::{ConfirmMatch, MatchSignal, ProposeMatch, RuleMatch, Split};

    pub(super) use crate::bind;
    pub(super) use crate::domain::Vocabulary;
    pub(super) use crate::error::KernelError;
    pub(super) use crate::event::Event;
    pub(super) use crate::ids::{Group, Id, Identity, MatchCandidate, Person};
    pub(super) use crate::merge::*;
    pub(super) use crate::store::{Count, Reads, Sqlite, Store, Value};
    pub(super) use crate::testing::{Local, Registered, TEST_PASSWORD};

    /// Run a count query and return the number.
    pub(super) async fn count(harness: &Local, sql: &'static str, params: Vec<Value>) -> i64 {
        harness
            .store()
            .query::<Count>(sql, params)
            .await
            .expect("query runs")
            .first()
            .map_or(0, |row| row.0)
    }

    /// The person an identity currently resolves to.
    pub(super) async fn person(harness: &Local, identity: Id<Identity>) -> Id<Person> {
        person_of(&harness.store().reads(), identity)
            .await
            .expect("query runs")
            .expect("a registration always has a person")
    }

    /// Somebody's identity and person, from what registration returned.
    pub(super) fn who(registered: &Registered) -> (Id<Identity>, Id<Person>) {
        (
            registered.principal.identity().expect("a member"),
            registered.principal.acting_as().expect("a member"),
        )
    }

    /// A group party, since the command that creates one is K2's.
    pub(super) async fn group(harness: &Local, name: &'static str) -> Id<Group> {
        write(
            harness.store(),
            "INSERT INTO party (kind, display_name, status, created_at) \
             VALUES ('group', $1, 'active', $2)",
            bind![name, harness.store().now()],
        )
        .await;
        let rows: Vec<Count> = harness
            .store()
            .query(
                "SELECT id AS n FROM party WHERE kind = 'group' AND display_name = $1",
                bind![name],
            )
            .await
            .expect("query runs");
        Id::new(rows[0].0)
    }

    /// A membership row, likewise.
    pub(super) async fn join(harness: &Local, group: Id<Group>, party: Id<Person>, role: &str) {
        write(
            harness.store(),
            "INSERT INTO membership (group_id, party_id, role, at) VALUES ($1, $2, $3, $4)",
            bind![group, party, role, harness.store().now()],
        )
        .await;
    }

    /// A grant to a person, on some object.
    pub(super) async fn grant(harness: &Local, object: i64, relation: &str, subject: Id<Person>) {
        write(
            harness.store(),
            "INSERT OR IGNORE INTO relation \
             (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
             VALUES ('document', $1, $2, 'person', $3, NULL, $4)",
            bind![object, relation, subject, harness.store().now()],
        )
        .await;
    }

    /// The relations a person holds, as `object_id:relation` strings.
    pub(super) async fn grants(harness: &Local, subject: Id<Person>) -> Vec<String> {
        #[derive(Debug)]
        struct Grant(String);
        impl crate::store::FromRow for Grant {
            fn from_row(
                row: &mut impl crate::store::Cursor,
            ) -> Result<Self, crate::store::RowError> {
                Ok(Self(row.text("g")?))
            }
        }
        harness
            .store()
            .query::<Grant>(
                "SELECT object_id || ':' || relation AS g FROM relation \
                 WHERE subject_kind = 'person' AND subject_id = $1 ORDER BY g",
                bind![subject],
            )
            .await
            .expect("query runs")
            .into_iter()
            .map(|g| g.0)
            .collect()
    }

    /// The groups a party belongs to, as `group:role` strings.
    pub(super) async fn memberships(harness: &Local, party: Id<Person>) -> Vec<String> {
        #[derive(Debug)]
        struct Row(String);
        impl crate::store::FromRow for Row {
            fn from_row(
                row: &mut impl crate::store::Cursor,
            ) -> Result<Self, crate::store::RowError> {
                Ok(Self(row.text("g")?))
            }
        }
        harness
            .store()
            .query::<Row>(
                "SELECT group_id || ':' || role AS g FROM membership WHERE party_id = $1 \
                 ORDER BY g",
                bind![party],
            )
            .await
            .expect("query runs")
            .into_iter()
            .map(|r| r.0)
            .collect()
    }

    /// Queue a pair by hand, at the score its signal carries.
    pub(super) async fn queue(
        harness: &Local,
        a: Id<Identity>,
        b: Id<Identity>,
        signal: MatchSignal,
    ) -> Id<MatchCandidate> {
        harness
            .store()
            .execute(
                PROPOSE_FOR_TESTS,
                proposal_for_tests(a, b, signal, harness.store().now()),
            )
            .await
            .expect("the queue accepts a proposal");
        let (low, high) = (a.min(b), a.max(b));
        let rows: Vec<Count> = harness
            .store()
            .query(
                "SELECT id AS n FROM match_candidate \
                 WHERE identity_a = $1 AND identity_b = $2 AND signal = $3",
                bind![low, high, signal.as_str()],
            )
            .await
            .expect("query runs");
        Id::new(rows[0].0)
    }

    pub(super) use crate::merge::candidate::{
        PROPOSE_QUIETLY as PROPOSE_FOR_TESTS, proposal as proposal_for_tests,
    };

    /// An `oidc` factor, verified. The only factor kind two identities may
    /// currently share — `factor_email_unique_idx` makes an address unique
    /// across the deployment — so it is what the shared-factor paths are
    /// proved with. No command produces one; this is a row, not a fixture for
    /// a behaviour.
    pub(super) async fn shared_subject(harness: &Local, identity: Id<Identity>, subject: &str) {
        write(
            harness.store(),
            "INSERT INTO factor (identity_id, kind, value, verified_at, created_at) \
             VALUES ($1, 'oidc', $2, $3, $3)",
            bind![identity, subject, harness.store().now()],
        )
        .await;
    }

    /// Make a person a platform operator.
    pub(super) async fn operator(harness: &Local, person: Id<Person>) {
        crate::merge::rule::make_operator(harness.store(), person)
            .await
            .expect("the relation row is written");
    }

    /// Run a statement that must succeed.
    pub(super) async fn write(store: &Store<Sqlite>, sql: &'static str, params: Vec<Value>) {
        store
            .execute(sql, params)
            .await
            .expect("the statement runs");
    }

    /// A public id for a command's arguments.
    pub(super) fn public<T: crate::ids::Public>(
        harness: &Local,
        id: Id<T>,
    ) -> rn_api::ids::PublicId {
        id.public(harness.store().ids())
    }
}
