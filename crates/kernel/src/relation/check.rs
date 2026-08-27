//! `check()`, `list_visible()` and `visible_contacts()` — the three read paths
//! every authorisation in the system goes through.
//!
//! All three are one statement each, and all three are keyed on the subject
//! side. A principal expands into a handful of subject keys once per request
//! (`principal::expand`); those keys arrive here as a JSON array, `json_each`
//! turns them into rows, and the index migration 2 adds turns each of them
//! into a seek. Nothing here scans a table, and `explain_*` in
//! `tests/relations.rs` is what keeps it that way.

use crate::error::Outcome;
use crate::ids::{Group, Id, Person};
use crate::principal::{Principal, SubjectSet};
use crate::resource::ResourceRow;
use crate::store::{Cursor, FromRow, Reads, RowError};
use crate::{Timestamp, bind};

use super::vocabulary::{Relation, Vocabulary};
use super::{NO_IDS, Object, Subject, owner_ids, party_ids, subject_keys};

/// The authorisation query.
///
/// Two branches, one statement. The first is the relation store proper: the
/// principal's subject keys against `relation_subject_key_idx`, which carries
/// every column the branch reads, so it never visits the table. The second is
/// membership, which answers role questions on the two container kinds and is
/// handed an empty array — and so costs nothing — for every other kind.
///
/// `UNION ALL` rather than `UNION`: the caller is asking whether *any* row
/// covers what it wants, and de-duplicating first would be sorting rows nobody
/// is going to read.
pub const CHECK_SQL: &str = "SELECT r.relation AS relation \
     FROM json_each($1) s JOIN relation r ON r.subject_key = s.value \
     WHERE r.object_kind = $2 AND r.object_id = $3 \
     UNION ALL \
     SELECT m.role AS relation \
     FROM json_each($4) p JOIN membership m ON m.group_id = $3 AND m.party_id = p.value";

/// "Everything of this kind I can see", in one statement.
///
/// The inner `UNION` is the two ways a resource becomes visible — a relation
/// row naming one of my subjects, or my own ownership — and both sides are
/// index seeks. The outer select then pages the survivors by
/// `(created_at, id)`, which is the order `resource_kind_created_idx` is
/// already in, so there is no sort.
pub const LIST_VISIBLE_SQL: &str = "SELECT res.id, res.kind, res.owner_party_id, res.home_zone, \
            res.status, res.created_at \
     FROM resource res \
     WHERE res.kind = $1 AND res.status <> 'deleted' \
       AND (res.created_at < $2 OR (res.created_at = $2 AND res.id < $3)) \
       AND res.id IN (SELECT r.object_id FROM json_each($4) s \
                        JOIN relation r ON r.subject_key = s.value \
                       WHERE r.object_kind = $1 \
                      UNION \
                      SELECT o.id FROM json_each($5) p \
                        JOIN resource o ON o.owner_party_id = p.value AND o.kind = $1) \
     ORDER BY res.created_at DESC, res.id DESC LIMIT $6";

/// Whose contact details are visible inside one group.
///
/// `person:P #contact @group:G` is the only way one person's details reach
/// another, so this query *is* the contact-scoping rule: a person who is not
/// on a row of this shape for a group the caller belongs to is not returned,
/// whatever else the two share.
pub const CONTACTS_SQL: &str = "SELECT r.object_id AS id FROM relation r \
     WHERE r.subject_key = $1 AND r.object_kind = 'person' AND r.relation = 'contact' \
     ORDER BY r.object_id";

/// One page of a listing: where to resume, and how much to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    /// The `(created_at, id)` of the last row of the previous page. `None`
    /// starts at the newest.
    pub after: Option<(Timestamp, i64)>,
    /// How many rows to return.
    pub limit: usize,
}

impl Page {
    /// How many rows a page holds when nobody says otherwise.
    pub const DEFAULT_LIMIT: usize = 50;

    /// The most a single page will return, whatever a caller asks for.
    pub const MAX_LIMIT: usize = 500;

    /// The first page.
    pub const fn first(limit: usize) -> Self {
        Self { after: None, limit }
    }

    /// The page after `row`.
    pub const fn after(row: &ResourceRow, limit: usize) -> Self {
        Self {
            after: Some((row.created_at, row.id.get())),
            limit,
        }
    }

    /// The cursor as the statement binds it: the newest possible position when
    /// there is no cursor yet.
    const fn cursor(self) -> (Timestamp, i64) {
        match self.after {
            Some(at) => at,
            None => (i64::MAX, i64::MAX),
        }
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::first(Self::DEFAULT_LIMIT)
    }
}

/// One relation word, as the check query returns it.
struct Held(Option<Relation>);

impl FromRow for Held {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(Relation::parse(&row.text("relation")?)))
    }
}

/// May a principal — already expanded — do `relation` to `object`?
///
/// One indexed query, then nesting in code: a row saying `editor` answers a
/// question about `viewer`, and a row saying `owner` answers every question
/// about the thing it owns.
pub async fn check(
    store: &impl Reads,
    subjects: &SubjectSet,
    relation: Relation,
    object: Object,
) -> Outcome<bool> {
    // A kind that admits nothing of this shape cannot be reached however many
    // rows exist. Asking the database first would make a typo in a kind name
    // an expensive way to learn nothing.
    if !Vocabulary::KERNEL
        .kind(object.kind)
        .is_some_and(|kind| kind.admits_relation(relation))
    {
        return Ok(false);
    }
    let containers = Vocabulary::KERNEL
        .kind(object.kind)
        .is_some_and(|kind| kind.is_container);
    let parties = if containers {
        party_ids(subjects)
    } else {
        NO_IDS.to_owned()
    };

    let held = store
        .query::<Held>(
            CHECK_SQL,
            bind![subject_keys(subjects), object.kind, object.id, parties],
        )
        .await?;
    Ok(held
        .into_iter()
        .any(|row| row.0.is_some_and(|held| held.covers(relation))))
}

/// [`check`], for a caller holding a principal rather than its expansion.
///
/// Expanding costs a query, so a request that asks more than one question
/// expands once and calls [`check`]. This is the single-question form.
pub async fn check_principal(
    store: &impl Reads,
    principal: &Principal,
    relation: Relation,
    object: Object,
) -> Outcome<bool> {
    let subjects = crate::principal::expand(store, principal).await?;
    check(store, &subjects, relation, object).await
}

/// Everything of one kind a principal can see, newest first.
pub async fn list_visible(
    store: &impl Reads,
    subjects: &SubjectSet,
    kind: &'static str,
    page: Page,
) -> Outcome<Vec<ResourceRow>> {
    let (created_at, id) = page.cursor();
    let limit = page.limit.clamp(1, Page::MAX_LIMIT) as i64;
    Ok(store
        .query::<ResourceRow>(
            LIST_VISIBLE_SQL,
            bind![
                kind,
                created_at,
                id,
                subject_keys(subjects),
                owner_ids(subjects),
                limit
            ],
        )
        .await?)
}

/// One person id, as the contacts query returns it.
struct ContactId(Id<Person>);

impl FromRow for ContactId {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.id("id")?))
    }
}

/// Whose contact details this principal may see inside `group`.
///
/// Empty for a principal that does not belong to the group — not a decline,
/// because this is a query and the answer to "who can I see in a group I am
/// not in" is genuinely nobody.
pub async fn visible_contacts(
    store: &impl Reads,
    subjects: &SubjectSet,
    group: Id<Group>,
) -> Outcome<Vec<Id<Person>>> {
    if !subjects.groups.contains(&group) && !subjects.organizations.contains(&Id::new(group.get()))
    {
        return Ok(Vec::new());
    }
    let rows = store
        .query::<ContactId>(CONTACTS_SQL, bind![Subject::group(group).key()])
        .await?;
    Ok(rows.into_iter().map(|row| row.0).collect())
}
