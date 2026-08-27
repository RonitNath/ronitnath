//! Properties a merge must hold for every access graph, not only the ones a
//! hand-written case happened to draw.
//!
//! Registration hashes a password with argon2id, which is tens of milliseconds
//! by design, so the case counts here are small and the graphs are the part
//! that varies. What is being proved is structural — access is preserved, a
//! split restores what it should, a signal alone decides nothing — and those
//! do not need a thousand cases to fail when they are wrong.

use proptest::prelude::*;
use rn_api::commands::{ConfirmMatch, CreateDocument, MatchSignal, Split, Transfer};
use rn_kernel::domain::Vocabulary;
use rn_kernel::error::KernelError;
use rn_kernel::event::Event;
use rn_kernel::ids::{Id, Identity, MatchCandidate, Person, Resource};
use rn_kernel::merge::{self, candidate};
use rn_kernel::principal::Principal;
use rn_kernel::relation::{self, Object, Page, Relation};
use rn_kernel::resource::kinds;
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError, Value};
use rn_kernel::testing::{Local, Registered};
use rn_kernel::{bind, cmd, principal};

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
                    other_email: None,
                    other_password: None,
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
                    other_email: None,
                    other_password: None,
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
                    other_email: None,
                    other_password: None,
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
            // O1, the OpenID Provider. Every one of these is a *kernel*
            // column and names a person on purpose, because what an OpenID
            // client is told about is a human rather than a registration: the
            // client it owns, the service party it speaks as, whose code and
            // whose token, whose consent, and whose `sub`.
            //
            // Merging stays cheap all the same, and each one says how.
            // `oidc_subject` is read through `person_alias`, so both subs
            // resolve to the survivor without a row moving. A consent is a
            // relation row, and merge unions those onto the survivor;
            // `oidc_consent` is the scope attribute beside it, and an
            // absorbed person's row not moving means the survivor is asked to
            // consent again — the safe direction. A code and a token are
            // bound to a *session*, and a session binds an identity, which is
            // exactly what a merge leaves alone.
            "oidc_client.owner_party_id",
            "oidc_client.service_party_id",
            "oidc_code.person_id",
            "oidc_consent.person_id",
            "oidc_subject.person_id",
            "oidc_token.person_id",
            "oidc_token.service_party_id",
            // The handles a merge absorbed, which keep resolving to the
            // survivor and stay unavailable to anybody else.
            "party_handle_alias.person_id",
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

// ------------------------------------------------- ownership and the person --

/// A document owned by whoever is acting, and its resource id.
async fn document(harness: &Local, owner: &Principal, title: &str) -> Id<Resource> {
    let committed = cmd::create_document(
        &harness.ctx(owner.clone()),
        &CreateDocument {
            title: title.to_owned(),
            body: String::new(),
            owner: None,
        },
    )
    .await
    .expect("the owner may create a document");
    let Event::DocumentCreated { document, .. } = committed.event else {
        panic!("creating a document produces a create-document event");
    };
    document
}

/// The party on the resource row.
async fn owner_of(harness: &Local, resource: Id<Resource>) -> i64 {
    let rows: Vec<Count> = harness
        .store()
        .query(
            "SELECT owner_party_id AS n FROM resource WHERE id = $1",
            bind![resource],
        )
        .await
        .expect("query runs");
    rows[0].0
}

/// The `#owner` relation rows on a document, as the person ids that hold them.
async fn owner_rows(harness: &Local, resource: Id<Resource>) -> Vec<String> {
    rows(
        harness,
        "SELECT subject_kind || ':' || subject_id AS t FROM relation \
         WHERE object_kind = 'document' AND object_id = $1 AND relation = 'owner' ORDER BY t",
        bind![resource],
    )
    .await
}

/// Merge two self-registered people, the way a person proves it themselves.
async fn merge_them(harness: &Local, first: &Registered, second: &Registered) {
    let (a, _) = who(first);
    let (b, _) = who(second);
    let candidate = queue(harness, a, b, MatchSignal::NameAndGroup).await;
    let signed_in = principal::resolve(harness.store(), &first.token)
        .await
        .expect("resolves");
    merge::confirm_match(
        &harness.ctx(signed_in.principal),
        &ConfirmMatch {
            candidate: candidate.public(harness.store().ids()),
            other_session: Some(second.token.expose().to_owned()),
            other_email: None,
            other_password: None,
        },
    )
    .await
    .expect("two live sessions are proof");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4))]

    /// Ownership follows the person.
    ///
    /// After a merge the survivor is not merely *allowed* to reach what the
    /// absorbed person owned — it owns it. The two ways that has to show are
    /// the two a caller ever sees: `list_visible` returns the rows, and
    /// `check(survivor, owner, r)` answers yes. They are asserted separately
    /// because they read different things — the `resource` column and the
    /// `#owner` relation row — and a merge that moved one without the other
    /// would pass whichever half somebody happened to write.
    #[test]
    fn a_merged_persons_resources_are_owned_by_the_survivor(
        mine in 0usize..3,
        theirs in 1usize..3,
    ) {
        block(async move {
            let harness = Local::new();
            let first = harness.register("Ronit", "owns-a@example.test").await.expect("registers");
            let second = harness.register("Ronit", "owns-b@example.test").await.expect("registers");
            let (_, person_a) = who(&first);
            let (_, person_b) = who(&second);

            let mut all = Vec::new();
            for n in 0..mine {
                all.push(document(&harness, &first.principal, &format!("mine {n}")).await);
            }
            let mut absorbed = Vec::new();
            for n in 0..theirs {
                absorbed.push(document(&harness, &second.principal, &format!("theirs {n}")).await);
            }
            for doc in &absorbed {
                prop_assert_eq!(owner_of(&harness, *doc).await, person_b.get());
            }
            all.extend(absorbed.iter().copied());

            merge_them(&harness, &first, &second).await;

            // The column moved.
            for doc in &all {
                prop_assert_eq!(
                    owner_of(&harness, *doc).await,
                    person_a.get(),
                    "a resource is still owned by a party nobody can act as"
                );
            }

            // And both read paths agree with it.
            let signed_in = principal::resolve(harness.store(), &first.token)
                .await
                .expect("resolves");
            let subjects = principal::expand(harness.store(), &signed_in.principal)
                .await
                .expect("expands");
            let listed = relation::list_visible(
                harness.store(),
                &subjects,
                kinds::DOCUMENT,
                Page::first(50),
            )
            .await
            .expect("lists");
            let seen: std::collections::BTreeSet<i64> =
                listed.iter().map(|row| row.id.get()).collect();
            for doc in &all {
                prop_assert!(seen.contains(&doc.get()), "list_visible lost a merged resource");
                prop_assert!(
                    relation::check(
                        harness.store(),
                        &subjects,
                        Relation::Owner,
                        Object::document(*doc),
                    )
                    .await
                    .expect("checks"),
                    "the survivor does not hold #owner on a resource it owns"
                );
            }
            Ok(())
        })?;
    }
}

/// What a split gives back of ownership, and what it leaves alone.
///
/// Three resources, three fates: the one the absorbed person owned goes back,
/// the one the survivor owned all along stays, and the one the survivor
/// transferred on while the merge stood stays with whoever holds it now. The
/// last is the interesting one — a split undoes a merge, not every decision
/// taken while it stood.
#[tokio::test]
async fn a_split_hands_back_the_resources_the_person_arrived_owning() {
    let harness = Local::new();
    let first = harness
        .register("Ronit", "split-owner-a@example.test")
        .await
        .expect("registers");
    let second = harness
        .register("Ronit", "split-owner-b@example.test")
        .await
        .expect("registers");
    let third = harness
        .register("Abeer", "split-owner-c@example.test")
        .await
        .expect("registers");
    let (_, person_a) = who(&first);
    let (b, person_b) = who(&second);
    let (_, person_c) = who(&third);

    let ours = document(&harness, &first.principal, "the survivor's own").await;
    let theirs = document(&harness, &second.principal, "brought to the merge").await;
    let moved_on = document(&harness, &second.principal, "transferred away after").await;

    merge_them(&harness, &first, &second).await;
    assert_eq!(owner_of(&harness, theirs).await, person_a.get());

    // While the merge stands, the survivor hands one of them on. That is a
    // decision somebody took, and the split must not reverse it.
    let signed_in = principal::resolve(harness.store(), &first.token)
        .await
        .expect("resolves");
    cmd::transfer(
        &harness.ctx(signed_in.principal.clone()),
        &Transfer {
            resource: moved_on.public(harness.store().ids()),
            to: person_c.public(harness.store().ids()),
        },
    )
    .await
    .expect("the owner may transfer");

    let committed = merge::split(
        &harness.ctx(signed_in.principal),
        &Split {
            identity: b.public(harness.store().ids()),
            evidence: "merged in error".into(),
        },
    )
    .await
    .expect("splits");
    let Event::PersonSplit { person: fresh, .. } = committed.event else {
        panic!("a split produces a split event");
    };

    assert_eq!(
        owner_of(&harness, theirs).await,
        fresh.get(),
        "the resource the identity arrived owning did not go back"
    );
    let mut expected = vec![
        // The absorbed person's own row, which the merge kept as the record of
        // what it held — and which is how the split found this resource.
        format!("person:{}", person_b.get()),
        format!("person:{}", fresh.get()),
    ];
    expected.sort();
    assert_eq!(
        owner_rows(&harness, theirs).await,
        expected,
        "the survivor still claims to own what it handed back"
    );
    assert_eq!(
        owner_of(&harness, ours).await,
        person_a.get(),
        "the survivor's own resource followed the split"
    );
    assert_eq!(
        owner_of(&harness, moved_on).await,
        person_c.get(),
        "a split reversed a transfer taken while the merge stood"
    );
}

/// The limit the module doc states, said as a test: attribution is per
/// *person*, never per identity.
///
/// A person made of two identities is absorbed. Splitting one of them out
/// hands that one everything the former person owned, including what the
/// other identity brought, because nothing in the schema records which
/// identity of a party created a resource — `resource.owner_party_id` names a
/// party and that is the whole of it. Sorting the rest out is `Transfer`.
#[tokio::test]
async fn a_split_cannot_tell_which_identity_of_a_person_brought_a_resource() {
    let harness = Local::new();
    let anchor = harness
        .register("Ronit", "attr-anchor@example.test")
        .await
        .expect("registers");
    let left = harness
        .register("Ronit", "attr-left@example.test")
        .await
        .expect("registers");
    let right = harness
        .register("Ronit", "attr-right@example.test")
        .await
        .expect("registers");

    let from_left = document(&harness, &left.principal, "left brought this").await;
    let from_right = document(&harness, &right.principal, "right brought this").await;

    // Two identities become one person, and that person is then absorbed.
    merge_them(&harness, &left, &right).await;
    merge_them(&harness, &anchor, &left).await;

    let (_, person_anchor) = who(&anchor);
    assert_eq!(owner_of(&harness, from_left).await, person_anchor.get());
    assert_eq!(owner_of(&harness, from_right).await, person_anchor.get());

    let (right_identity, _) = who(&right);
    let signed_in = principal::resolve(harness.store(), &anchor.token)
        .await
        .expect("resolves");
    let committed = merge::split(
        &harness.ctx(signed_in.principal),
        &Split {
            identity: right_identity.public(harness.store().ids()),
            evidence: "the right-hand registration was not me".into(),
        },
    )
    .await
    .expect("splits");
    let Event::PersonSplit { person: fresh, .. } = committed.event else {
        panic!("a split produces a split event");
    };

    assert_eq!(owner_of(&harness, from_right).await, fresh.get());
    assert_eq!(
        owner_of(&harness, from_left).await,
        fresh.get(),
        "attribution is per person: the split takes everything that person owned"
    );
}
