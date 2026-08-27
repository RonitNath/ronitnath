//! The relation store's gates: every hot query's plan, and the properties the
//! model has to hold for every input rather than for the cases somebody
//! thought of.

use proptest::prelude::*;
use rn_api::commands::{CreateDocument, Share, Transfer};
use rn_api::whoami::DocRole;
use rn_kernel::domain::Vocabulary as _;
use rn_kernel::ids::{Group, Id, Person, Resource};
use rn_kernel::principal::{Principal, SubjectSet};
use rn_kernel::relation::{
    self, Object, Relation, Subject, SubjectKind, Vocabulary, visible_contacts,
};
use rn_kernel::resource::kinds;
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError, RowId, Value};
use rn_kernel::testing::Local;
use rn_kernel::{bind, cmd, invite, org};

/// Each case runs on its own current-thread runtime and its own database, so
/// two cases can never share a world.
fn block<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime starts")
        .block_on(future)
}

// ------------------------------------------------------------- fixtures ---

async fn party(harness: &Local, kind: &'static str) -> i64 {
    harness
        .store()
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) \
             VALUES ($1, 'A party', 'active', 1)",
            bind![kind],
        )
        .await
        .expect("the party lands");
    harness
        .store()
        .query::<Count>("SELECT max(id) AS n FROM party", bind![])
        .await
        .expect("reads")[0]
        .0
}

async fn document_row(harness: &Local, owner: i64, at: i64) -> i64 {
    harness
        .store()
        .query::<RowId<Resource>>(
            rn_kernel::resource::INSERT_SQL,
            bind![kinds::DOCUMENT, owner, "us-west", "published", at],
        )
        .await
        .expect("the resource lands")[0]
        .0
        .get()
}

fn set(person: i64, groups: &[i64]) -> SubjectSet {
    SubjectSet {
        identity: Some(Id::new(1)),
        person: Some(Id::new(person)),
        organizations: Vec::new(),
        groups: groups.iter().map(|g| Id::new(*g)).collect(),
        link: None,
        authenticated: true,
    }
}

// --------------------------------------------------------------- explain ---

/// Assert a plan reaches the named index and scans nothing but its own
/// arguments. `json_each` is the principal's expanded subject set — a list the
/// query was handed, not a table it went looking through — and a co-routine
/// is the bounded output of a subquery this statement already constrained.
fn rides(plan: &[String], index: &str) {
    let joined = plan.join("\n");
    assert!(
        joined.contains(index),
        "the plan does not use {index}:\n{joined}"
    );
    for line in plan {
        assert!(
            !line.contains("SCAN")
                || line.contains("VIRTUAL TABLE")
                || line.contains("SCAN (subquery")
                || line.contains("CONSTANT ROW"),
            "a full scan crept into a hot query:\n{joined}"
        );
    }
}

fn plan(harness: &Local, sql: &str, params: Vec<Value>) -> Vec<String> {
    harness
        .store()
        .engine()
        .explain(sql, params)
        .expect("EXPLAIN QUERY PLAN runs")
}

#[tokio::test]
async fn explain_check_seeks_once_per_subject_and_once_per_membership() {
    let harness = Local::new();
    let keys = r#"["public:0","authenticated:0","person:1","group:7"]"#;
    let plan = plan(
        &harness,
        relation::CHECK_SQL,
        bind![keys, "document", 3i64, "[1,7]"],
    );
    // The whole point of the generated `subject_key` column: one seek per
    // subject, whatever the object's fan-out is.
    rides(&plan, "relation_subject_key_idx");
    rides(&plan, "sqlite_autoindex_membership_1");
    assert!(
        plan.join("\n").contains("object_id=?"),
        "the seek must reach the object, not filter for it:\n{}",
        plan.join("\n")
    );
}

#[tokio::test]
async fn explain_list_visible_seeks_both_ways_a_resource_becomes_visible() {
    let harness = Local::new();
    let keys = r#"["public:0","person:1","group:7"]"#;
    // In the order the statement names them: kind, the two cursor columns,
    // the owner ids, the limit, the subject keys. SQLite assigns `$1` an
    // index by first appearance, so this order is the statement's, not a
    // convention.
    let plan = plan(
        &harness,
        relation::LIST_VISIBLE_SQL,
        bind!["document", i64::MAX, i64::MAX, "[1]", 50i64, keys],
    );
    rides(&plan, "resource_kind_created_idx");
    rides(&plan, "relation_subject_key_idx");
    // The ownership branch takes the newest page and stops, which is what the
    // index carrying `(created_at, id)` is for (migration 5).
    rides(&plan, "resource_owner_created_idx");
}

#[tokio::test]
async fn explain_visible_contacts_rides_the_subject_key_index() {
    let harness = Local::new();
    let plan = plan(&harness, relation::CONTACTS_SQL, bind!["group:7"]);
    rides(&plan, "relation_subject_key_idx");
}

#[tokio::test]
async fn explain_an_invitation_lookup_is_two_seeks() {
    let harness = Local::new();
    let plan = plan(&harness, invite::LOOKUP_SQL, bind![vec![0u8; 32]]);
    // A public page anybody can post a guess to: the link by its digest, then
    // what the link is worth.
    rides(&plan, "sqlite_autoindex_link_1");
    rides(&plan, "relation_subject_idx");
}

#[tokio::test]
async fn explain_a_role_lookup_is_the_membership_primary_key() {
    let harness = Local::new();
    let plan = plan(&harness, org::ROLE_SQL, bind![1i64, 2i64]);
    rides(&plan, "sqlite_autoindex_membership_1");
    assert!(
        plan.join("\n").contains("party_id=?"),
        "both halves of the key, or it is a scan of the container:\n{}",
        plan.join("\n")
    );
}

// ------------------------------------------------------------ properties ---

/// The relations a `Share` can name, and the subjects it can name them to.
fn doc_role() -> impl Strategy<Value = DocRole> {
    prop_oneof![
        Just(DocRole::Viewer),
        Just(DocRole::Commenter),
        Just(DocRole::Editor)
    ]
}

fn relation() -> impl Strategy<Value = Relation> {
    (0usize..Relation::ALL.len()).prop_map(|i| Relation::ALL[i])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// A grant of one relation answers a question about another exactly when
    /// the nesting says it does — never more, never less.
    #[test]
    fn check_answers_exactly_what_the_nesting_says(
        granted in relation(),
        asked in relation(),
    ) {
        block(async {
            let harness = Local::new();
            let owner = party(&harness, "person").await;
            let doc = document_row(&harness, owner, 1).await;
            let object = Object::document(Id::new(doc));
            let reader = party(&harness, "person").await;

            let admitted = Vocabulary::KERNEL.admits(
                kinds::DOCUMENT, granted, SubjectKind::Person);
            if admitted {
                relation::grant(
                    harness.store(),
                    object,
                    granted,
                    Subject::person(Id::new(reader)),
                )
                .await
                .expect("grants");
            }

            let answer = relation::check(
                harness.store(), &set(reader, &[]), asked, object)
                .await
                .expect("checks");
            let wanted = admitted
                && Vocabulary::KERNEL.admits(kinds::DOCUMENT, asked, SubjectKind::Person)
                && granted.covers(asked);
            prop_assert_eq!(
                answer, wanted,
                "granted {:?}, asked {:?}", granted, asked
            );
            Ok(())
        })?;
    }

    /// `Share` writes a row exactly when the kind's vocabulary admits it. A
    /// subject kind the kind refuses is refused however the command is dressed.
    #[test]
    fn no_row_outside_the_vocabulary_is_ever_written(
        role in doc_role(),
        subject_kind in 0usize..3,
        on_container in any::<bool>(),
    ) {
        block(async {
            let harness = Local::new();
            let ronit = harness
                .register("Ronit", "vocab@example.test")
                .await
                .expect("registers");
            let key = harness.store().ids();

            let (org, org_resource) = found(&harness, &ronit.principal).await;
            let doc = make_document(&harness, &ronit.principal).await;
            let group = make_group(&harness, &ronit.principal).await;

            let subject = match subject_kind {
                0 => person_of(&ronit.principal).public(key),
                1 => org.public(key),
                _ => group.public(key),
            };
            let resource = if on_container { org_resource } else { doc };

            let outcome = cmd::share(
                &harness.ctx(ronit.principal.clone()),
                &Share {
                    resource: resource.public(key),
                    subject,
                    relation: role,
                },
            )
            .await;

            let expected = !on_container;
            prop_assert_eq!(
                outcome.is_ok(), expected,
                "an organization is not shared by its resource id"
            );

            // Whatever happened, every row in the table is one the vocabulary
            // admits — which is the invariant the command exists to keep.
            for grant in every_grant(&harness).await {
                prop_assert!(
                    Vocabulary::KERNEL.admits(
                        &grant.0, grant.1, grant.2),
                    "{:?} is outside the vocabulary", grant
                );
            }
            Ok(())
        })?;
    }

    /// Groups are granted, never owners — whatever sequence of commands runs.
    #[test]
    fn a_group_never_becomes_an_owner(to_group in any::<bool>()) {
        block(async {
            let harness = Local::new();
            let ronit = harness
                .register("Ronit", "owner@example.test")
                .await
                .expect("registers");
            let key = harness.store().ids();
            let doc = make_document(&harness, &ronit.principal).await;
            let group = make_group(&harness, &ronit.principal).await;
            let (_, org_resource) = found(&harness, &ronit.principal).await;

            let target = if to_group {
                group.public(key)
            } else {
                person_of(&ronit.principal).public(key)
            };
            let moved = cmd::transfer(
                &harness.ctx(ronit.principal.clone()),
                &Transfer { resource: doc.public(key), to: target },
            )
            .await;
            prop_assert!(moved.is_err(), "a group and a self-transfer are both refused");

            let _ = org_resource;
            prop_assert_eq!(group_owned(&harness).await, 0);
            Ok(())
        })?;
    }

    /// No command but `Transfer` moves `resource.owner_party_id`. (Merge and
    /// `Split` move it too, but they change which party a person *is* rather
    /// than who the owner is; `tests/merge_properties.rs` holds that pair.)
    #[test]
    fn transfer_is_the_only_command_that_changes_an_owner(
        role in doc_role(),
        publish in any::<bool>(),
    ) {
        block(async {
            let harness = Local::new();
            let ronit = harness
                .register("Ronit", "keeps@example.test")
                .await
                .expect("registers");
            let abeer = harness
                .register("Abeer", "takes@example.test")
                .await
                .expect("registers");
            let key = harness.store().ids();
            let doc = make_document(&harness, &ronit.principal).await;
            let before = owner_of(&harness, doc).await;

            let _ = cmd::share(
                &harness.ctx(ronit.principal.clone()),
                &Share {
                    resource: doc.public(key),
                    subject: person_of(&abeer.principal).public(key),
                    relation: role,
                },
            )
            .await;
            if publish {
                let _ = cmd::publish_document(
                    &harness.ctx(ronit.principal.clone()),
                    &rn_api::commands::PublishDocument { document: doc.public(key) },
                )
                .await;
            }
            prop_assert_eq!(owner_of(&harness, doc).await, before, "sharing is not giving");

            cmd::transfer(
                &harness.ctx(ronit.principal.clone()),
                &Transfer {
                    resource: doc.public(key),
                    to: person_of(&abeer.principal).public(key),
                },
            )
            .await
            .expect("the owner may");
            prop_assert_eq!(
                owner_of(&harness, doc).await,
                person_of(&abeer.principal).get()
            );
            Ok(())
        })?;
    }

    /// Contact details never cross a group boundary. Random people, random
    /// groups, random contact grants: what comes back is always a person with
    /// a `contact` row into a group the caller is actually in.
    #[test]
    fn a_contact_never_leaks_out_of_the_group_it_was_scoped_to(
        shares in prop::collection::vec((0usize..6, 0usize..3), 0..12),
        caller in 0usize..6,
        member_of in prop::collection::vec(0usize..3, 0..3),
    ) {
        block(async {
            let harness = Local::new();
            let mut people = Vec::new();
            for _ in 0..6 {
                people.push(party(&harness, "person").await);
            }
            let mut groups = Vec::new();
            for _ in 0..3 {
                groups.push(party(&harness, "group").await);
            }
            for (who, group) in &shares {
                relation::grant(
                    harness.store(),
                    Object::person(Id::new(people[*who])),
                    Relation::Contact,
                    Subject::group(Id::new(groups[*group])),
                )
                .await
                .expect("grants");
            }

            let mine: Vec<i64> = member_of.iter().map(|g| groups[*g]).collect();
            let subjects = set(people[caller], &mine);
            for (index, group) in groups.iter().enumerate() {
                let seen = visible_contacts(
                    harness.store(), &subjects, Id::<Group>::new(*group))
                    .await
                    .expect("lists");
                if !mine.contains(group) {
                    prop_assert!(seen.is_empty(), "a group I am not in shows nobody");
                    continue;
                }
                for person in seen {
                    prop_assert!(
                        shares.iter().any(|(who, g)| people[*who] == person.get()
                            && *g == index),
                        "a person with no contact row into this group came back"
                    );
                }
            }
            Ok(())
        })?;
    }
}

// ------------------------------------------------------------- helpers ----

fn person_of(principal: &Principal) -> Id<Person> {
    principal.acting_as().expect("a member acts as somebody")
}

async fn found(
    harness: &Local,
    principal: &Principal,
) -> (Id<rn_kernel::ids::Organization>, Id<Resource>) {
    let committed = cmd::create_organization(
        &harness.ctx(principal.clone()),
        &rn_api::commands::CreateOrganization {
            display_name: "Isoastra".to_owned(),
        },
    )
    .await
    .expect("founds");
    match committed.event {
        rn_kernel::Event::OrganizationCreated {
            organization,
            resource,
            ..
        } => (organization, resource),
        other => panic!("create-organization produced {other:?}"),
    }
}

async fn make_group(harness: &Local, principal: &Principal) -> Id<Group> {
    let committed = cmd::create_group(
        &harness.ctx(principal.clone()),
        &rn_api::commands::CreateGroup {
            display_name: "Team".to_owned(),
            organization: None,
        },
    )
    .await
    .expect("creates");
    match committed.event {
        rn_kernel::Event::GroupCreated { group, .. } => group,
        other => panic!("create-group produced {other:?}"),
    }
}

async fn make_document(harness: &Local, principal: &Principal) -> Id<Resource> {
    let committed = cmd::create_document(
        &harness.ctx(principal.clone()),
        &CreateDocument {
            title: "Kernel report".to_owned(),
            body: "Five rules".to_owned(),
            owner: None,
        },
    )
    .await
    .expect("creates");
    match committed.event {
        rn_kernel::Event::DocumentCreated { document, .. } => document,
        other => panic!("create-document produced {other:?}"),
    }
}

async fn owner_of(harness: &Local, resource: Id<Resource>) -> i64 {
    rn_kernel::resource::load(harness.store(), resource)
        .await
        .expect("reads")
        .expect("it is there")
        .owner_party_id
        .get()
}

/// How many resources a group owns. The answer is always zero.
async fn group_owned(harness: &Local) -> i64 {
    harness
        .store()
        .query::<Count>(
            "SELECT count(*) AS n FROM resource r JOIN party p ON p.id = r.owner_party_id \
             WHERE p.kind IN ('group', 'service')",
            bind![],
        )
        .await
        .expect("reads")[0]
        .0
}

/// Every row in the relation table, as the vocabulary sees it.
struct GrantRow {
    object_kind: String,
    relation: Relation,
    subject_kind: SubjectKind,
}

impl FromRow for GrantRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let relation = row.text("relation")?;
        let subject_kind = row.text("subject_kind")?;
        Ok(Self {
            object_kind: row.text("object_kind")?,
            relation: Relation::parse(&relation).expect("the trigger admits only these"),
            subject_kind: SubjectKind::parse(&subject_kind).expect("the CHECK admits only these"),
        })
    }
}

async fn every_grant(harness: &Local) -> Vec<(String, Relation, SubjectKind)> {
    harness
        .store()
        .query::<GrantRow>(
            "SELECT object_kind, relation, subject_kind FROM relation",
            bind![],
        )
        .await
        .expect("reads")
        .into_iter()
        .map(|row| (row.object_kind, row.relation, row.subject_kind))
        .collect()
}
