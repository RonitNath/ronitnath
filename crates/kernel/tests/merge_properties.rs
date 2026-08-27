//! Properties a merge must hold for every access graph, not only the ones a
//! hand-written case happened to draw.
//!
//! Registration hashes a password with argon2id, which is tens of milliseconds
//! by design, so the case counts here are small and the graphs are the part
//! that varies. What is being proved is structural — access is preserved, a
//! split restores what it should, a signal alone decides nothing — and those
//! do not need a thousand cases to fail when they are wrong.

use proptest::prelude::*;
use rn_api::commands::{ConfirmMatch, MatchSignal, Split};
use rn_kernel::domain::Vocabulary;
use rn_kernel::error::KernelError;
use rn_kernel::event::Event;
use rn_kernel::ids::{Id, Identity, MatchCandidate, Person};
use rn_kernel::merge::{self, candidate};
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError, Value};
use rn_kernel::testing::{Local, Registered};
use rn_kernel::{bind, principal};

fn block<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime starts")
        .block_on(future)
}

fn who(registered: &Registered) -> (Id<Identity>, Id<Person>) {
    (
        registered.principal.identity().expect("a member"),
        registered.principal.acting_as().expect("a member"),
    )
}

/// A grant or a membership, as a comparable string.
#[derive(Debug)]
struct Text(String);

impl FromRow for Text {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.text("t")?))
    }
}

async fn rows(harness: &Local, sql: &'static str, params: Vec<Value>) -> Vec<String> {
    harness
        .store()
        .query::<Text>(sql, params)
        .await
        .expect("query runs")
        .into_iter()
        .map(|row| row.0)
        .collect()
}

/// Everything this person may reach: `object:relation`, in order.
async fn access(harness: &Local, person: Id<Person>) -> Vec<String> {
    rows(
        harness,
        "SELECT object_kind || ':' || object_id || ':' || relation AS t FROM relation \
         WHERE subject_kind = 'person' AND subject_id = $1 ORDER BY t",
        bind![person],
    )
    .await
}

/// Everything this party belongs to: `group:role`, in order.
async fn memberships(harness: &Local, party: Id<Person>) -> Vec<String> {
    rows(
        harness,
        "SELECT group_id || ':' || role AS t FROM membership WHERE party_id = $1 ORDER BY t",
        bind![party],
    )
    .await
}

async fn grant(harness: &Local, object: i64, relation: &str, subject: Id<Person>) {
    harness
        .store()
        .execute(
            "INSERT OR IGNORE INTO relation \
             (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
             VALUES ('document', $1, $2, 'person', $3, NULL, $4)",
            bind![object, relation, subject, harness.store().now()],
        )
        .await
        .expect("the grant is written");
}

async fn group(harness: &Local, name: i64) -> Id<Person> {
    harness
        .store()
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) \
             VALUES ('group', 'g' || $1, 'active', $2)",
            bind![name, harness.store().now()],
        )
        .await
        .expect("the group is written");
    let rows: Vec<Count> = harness
        .store()
        .query(
            "SELECT id AS n FROM party WHERE kind = 'group' AND display_name = 'g' || $1",
            bind![name],
        )
        .await
        .expect("query runs");
    Id::new(rows[0].0)
}

async fn join(harness: &Local, group: Id<Person>, party: Id<Person>, role: &'static str) {
    harness
        .store()
        .execute(
            "INSERT OR IGNORE INTO membership (group_id, party_id, role, at) \
             VALUES ($1, $2, $3, $4)",
            bind![group, party, role, harness.store().now()],
        )
        .await
        .expect("the membership is written");
}

async fn queue(
    harness: &Local,
    a: Id<Identity>,
    b: Id<Identity>,
    signal: MatchSignal,
) -> Id<MatchCandidate> {
    harness
        .store()
        .execute(
            candidate::PROPOSE_QUIETLY,
            candidate::proposal(a, b, signal, harness.store().now()),
        )
        .await
        .expect("the queue accepts a proposal");
    let rows: Vec<Count> = harness
        .store()
        .query(
            "SELECT id AS n FROM match_candidate \
             WHERE identity_a = $1 AND identity_b = $2 AND signal = $3",
            bind![a.min(b), a.max(b), signal.as_str()],
        )
        .await
        .expect("query runs");
    Id::new(rows[0].0)
}

/// The roles and relations a case draws from. Two of each, nesting apart, is
/// enough to make "strongest wins" and "union" distinguishable.
const ROLES: [&str; 3] = ["member", "admin", "owner"];
const RELATIONS: [&str; 3] = ["viewer", "commenter", "editor"];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    /// Merge preserves the union of access.
    ///
    /// Whatever either person could reach before, the survivor reaches after —
    /// and nothing the absorbed person could not reach appears out of nowhere.
    #[test]
    fn a_merge_preserves_the_union_of_access(
        left in prop::collection::vec((1i64..4, 0usize..3), 0..5),
        right in prop::collection::vec((1i64..4, 0usize..3), 0..5),
        groups in prop::collection::vec((1i64..3, 0usize..3, prop::bool::ANY), 0..5),
    ) {
        block(async move {
            let harness = Local::new();
            let first = harness.register("Ronit", "left@example.test").await.expect("registers");
            let second = harness.register("Ronit", "right@example.test").await.expect("registers");
            let (_, person_a) = who(&first);
            let (b, person_b) = who(&second);

            for (object, relation) in &left {
                grant(&harness, *object, RELATIONS[*relation], person_a).await;
            }
            for (object, relation) in &right {
                grant(&harness, *object, RELATIONS[*relation], person_b).await;
            }
            for (name, role, on_the_left) in &groups {
                let group = group(&harness, *name).await;
                let party = if *on_the_left { person_a } else { person_b };
                join(&harness, group, party, ROLES[*role]).await;
            }

            let before: std::collections::BTreeSet<String> = access(&harness, person_a)
                .await
                .into_iter()
                .chain(access(&harness, person_b).await)
                .collect();

            let candidate = queue(&harness, who(&first).0, b, MatchSignal::NameAndGroup).await;
            merge::confirm_match(
                &harness.ctx(first.principal),
                &ConfirmMatch {
                    candidate: candidate.public(harness.store().ids()),
                    other_session: Some(second.token.expose().to_owned()),
                },
            )
            .await
            .expect("two live sessions are proof");

            let after: std::collections::BTreeSet<String> =
                access(&harness, person_a).await.into_iter().collect();
            prop_assert_eq!(&after, &before, "the survivor holds exactly the union");

            // And every membership of either side, at the strongest role the
            // two held between them.
            for entry in memberships(&harness, person_b).await {
                let (group, role) = entry.split_once(':').expect("group:role");
                let survivor: Vec<String> = memberships(&harness, person_a)
                    .await
                    .into_iter()
                    .filter(|m| m.starts_with(&format!("{group}:")))
                    .collect();
                prop_assert_eq!(survivor.len(), 1, "one membership per group");
                let held = survivor[0].split_once(':').expect("group:role").1.to_owned();
                let rank = |r: &str| ROLES.iter().position(|role| *role == r).unwrap_or(0);
                prop_assert!(
                    rank(&held) >= rank(role),
                    "the survivor kept the weaker role: {} < {}", held, role
                );
            }
            Ok(())
        })?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    /// `split ∘ merge` restores the membership set the identity arrived with.
    #[test]
    fn a_split_after_a_merge_restores_the_membership_set(
        theirs in prop::collection::vec((1i64..4, 0usize..3), 1..5),
    ) {
        block(async move {
            let harness = Local::new();
            let first = harness.register("Ronit", "keep@example.test").await.expect("registers");
            let second = harness.register("Ronit", "away@example.test").await.expect("registers");
            let (a, person_a) = who(&first);
            let (b, person_b) = who(&second);

            for (name, role) in &theirs {
                let group = group(&harness, *name).await;
                join(&harness, group, person_b, ROLES[*role]).await;
            }
            let before = memberships(&harness, person_b).await;

            let candidate = queue(&harness, a, b, MatchSignal::NameAndGroup).await;
            merge::confirm_match(
                &harness.ctx(first.principal),
                &ConfirmMatch {
                    candidate: candidate.public(harness.store().ids()),
                    other_session: Some(second.token.expose().to_owned()),
                },
            )
            .await
            .expect("merges");

            let signed_in = principal::resolve(harness.store(), &first.token)
                .await
                .expect("resolves");
            let committed = merge::split(
                &harness.ctx(signed_in.principal),
                &Split {
                    identity: b.public(harness.store().ids()),
                    evidence: "merged in error".into(),
                },
            )
            .await
            .expect("splits");

            let Event::PersonSplit { person, .. } = committed.event else {
                panic!("a split produces a split event");
            };
            prop_assert_ne!(person, person_a);
            prop_assert_eq!(memberships(&harness, person).await, before);
            Ok(())
        })?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5))]

    /// A signal is never proof, whichever signal it is and however it was
    /// raised. Confirming without a session on the other identity and without
    /// a factor both have verified is a decline, every time.
    #[test]
    fn no_signal_alone_merges_anybody(signal in 0usize..5) {
        block(async move {
            let harness = Local::new();
            let first = harness.register("Ronit", "weak-a@example.test").await.expect("registers");
            let second = harness.register("Ronit", "weak-b@example.test").await.expect("registers");
            let (a, _) = who(&first);
            let (b, person_b) = who(&second);
            let candidate = queue(&harness, a, b, MatchSignal::ALL[signal]).await;

            let refused = merge::confirm_match(
                &harness.ctx(first.principal),
                &ConfirmMatch {
                    candidate: candidate.public(harness.store().ids()),
                    other_session: None,
                },
            )
            .await;
            prop_assert!(matches!(refused, Err(KernelError::Decline(_))));

            let still = merge::person_of(&harness.store().reads(), b)
                .await
                .expect("query runs");
            prop_assert_eq!(still, Some(person_b), "nobody was merged");

            let open: Vec<Count> = harness
                .store()
                .query(
                    "SELECT count(*) AS n FROM match_candidate WHERE status = 'proposed'",
                    bind![],
                )
                .await
                .expect("query runs");
            prop_assert_eq!(open[0].0, 1, "the question is still open");
            Ok(())
        })?;
    }
}

/// The one invariant that is about the *schema* rather than about a command:
/// product tables reference an identity, never a person.
///
/// It is what makes merging humans cheap — no product row moves — and it is
/// the kind of rule that a future kind table breaks by accident, so it is
/// asserted against `pragma_foreign_key_list` rather than left as a sentence
/// in a report. Every column below that points at `party` is a kernel column,
/// and the list is exhaustive: a new one fails here until somebody says why.
#[tokio::test]
async fn no_table_outside_the_kernel_references_a_person() {
    let harness = Local::new();
    let references = rows(
        &harness,
        "SELECT m.name || '.' || fk.\"from\" AS t \
         FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) fk \
         WHERE m.type = 'table' AND fk.\"table\" = 'party' ORDER BY t",
        bind![],
    )
    .await;
    assert_eq!(
        references,
        vec![
            "audit.acting_as",
            "identity.person_id",
            "membership.group_id",
            "membership.party_id",
            // K2: the party row of an organization or group, bound to its resource.
            "party_resource.party_id",
            "person_alias.old_person_id",
            "person_alias.person_id",
            "person_link.from_person_id",
            "person_link.person_id",
            "resource.owner_party_id",
            "session.acting_as",
        ],
        "a table outside the kernel now references a party: product rows \
         reference an identity, never a person"
    );
}
