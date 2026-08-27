//! The member tier's document result sets: the documents a person can reach,
//! one of them in full, and the grants that put them there.
//!
//! Visibility is the kernel's, not this module's. A document is reachable two
//! ways — a relation row naming one of the principal's expanded subjects, or
//! the principal's own ownership — and both branches are the ones
//! [`list_visible`](rn_kernel::relation::list_visible) uses, over the same
//! indexes, so the list and an authorisation cannot answer differently. The
//! statement is written out rather than borrowed because the list carries a
//! title and a revision, which the resource registry knows nothing about.
//!
//! The single-document read is the one query in this bundle that takes a
//! parameter, so it is the one that can be *asked* about a row that is not the
//! reader's — and it declines, through
//! [`check`](rn_kernel::relation::check), rather than returning an empty list.
//! An empty list would be the honest answer to "which of mine is this" and the
//! wrong answer to "what is r_…", which is what was asked.

use rn_api::PublicId;
use rn_kernel::domain::{PartyKind, Vocabulary as _};
use rn_kernel::error::decline;
use rn_kernel::ids::{self, Id, IdKey, Person, Resource};
use rn_kernel::principal::SubjectSet;
use rn_kernel::relation::{Object, Relation, check, owner_ids, subject_keys};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::{Params, Row};
use crate::api::party::public_of;

/// How many documents one page of the list carries.
const PAGE: i64 = 200;

/// Every document this principal can reach, newest first.
///
/// The two arms of the `IN` are the two ways a document becomes visible, and
/// both are index seeks: the relation side rides
/// `relation_subject_key_idx`, the ownership side `resource_owner_idx`. The
/// `CROSS JOIN`s pin the loop order for the reason `check()` does — driven the
/// other way the relation side is a scan of every grant in the deployment.
pub const DOCUMENTS_SQL: &str = "SELECT res.id AS id, res.owner_party_id AS owner_id, res.status AS status, \
     res.created_at AS created_at, d.title AS title, d.draft_rev AS draft_rev, \
     d.published_rev AS published_rev, owner.kind AS owner_kind, \
     owner.display_name AS owner_display \
     FROM resource res \
     JOIN document d ON d.resource_id = res.id \
     JOIN party owner ON owner.id = res.owner_party_id \
     WHERE res.kind = 'document' AND res.status <> 'deleted' \
       AND res.id IN (SELECT r.object_id FROM json_each($1) s \
                        CROSS JOIN relation r ON r.subject_key = s.value \
                       WHERE r.object_kind = 'document' \
                      UNION \
                      SELECT o.id FROM json_each($2) p \
                        CROSS JOIN resource o ON o.owner_party_id = p.value \
                         AND o.kind = 'document') \
     ORDER BY res.created_at DESC, res.id DESC LIMIT $3";

/// One document, with its body. Addressed by its resource row's primary key.
pub const DOCUMENT_SQL: &str = "SELECT res.id AS id, res.owner_party_id AS owner_id, \
     res.status AS status, res.created_at AS created_at, d.title AS title, \
     d.body AS body, d.draft_rev AS draft_rev, d.published_rev AS published_rev, \
     owner.kind AS owner_kind, owner.display_name AS owner_display \
     FROM resource res \
     JOIN document d ON d.resource_id = res.id \
     JOIN party owner ON owner.id = res.owner_party_id \
     WHERE res.id = $1 AND res.kind = 'document' AND res.status <> 'deleted'";

/// Which relation this principal holds on each document they can reach.
///
/// The same index the visibility branch uses, read for its `relation` column:
/// what a reader may *do* with a row in the list is a property of the row, and
/// asking for it separately is one seek per subject rather than one per row.
pub const HELD_SQL: &str = "SELECT r.object_id AS id, r.relation AS relation \
     FROM json_each($1) s CROSS JOIN relation r ON r.subject_key = s.value \
     WHERE r.object_kind = 'document'";

/// Every grant on a set of documents — what a sharing panel renders.
///
/// Keyed on the object side, which is `relation_object_idx`, because this is
/// the one question in the member tier that is about a row rather than about
/// the reader. Only the three party subjects are returned: a document shared
/// with `public` or with a link is not something this tier can revoke, and a
/// subject with no public id could not be named back to the server anyway.
///
/// `owner` is not a share and is left out. It is the row `CreateDocument` and
/// `Transfer` keep in step with `resource.owner_party_id`, so offering it
/// beside the grants would offer a `Revoke` that takes a document away from
/// its owner while the column still says they own it.
pub const SHARES_SQL: &str = "SELECT r.object_id AS object_id, r.relation AS relation, \
     r.at AS at, p.kind AS party_kind, p.display_name AS display, p.id AS party_id \
     FROM json_each($1) d \
     CROSS JOIN relation r ON r.object_kind = 'document' AND r.object_id = d.value \
     JOIN party p ON p.id = r.subject_id \
     WHERE r.subject_kind IN ('person', 'organization', 'group') AND r.relation <> 'owner' \
     ORDER BY r.object_id, r.relation, p.display_name";

struct DocumentSummary {
    id: Id<Resource>,
    owner_id: i64,
    status: String,
    created_at: Timestamp,
    title: String,
    draft_rev: i64,
    published_rev: Option<i64>,
    owner_kind: String,
    owner_display: String,
}

impl FromRow for DocumentSummary {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            owner_id: row.int("owner_id")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
            title: row.text("title")?,
            draft_rev: row.int("draft_rev")?,
            published_rev: row.int_opt("published_rev")?,
            owner_kind: row.text("owner_kind")?,
            owner_display: row.text("owner_display")?,
        })
    }
}

struct Held {
    id: i64,
    relation: String,
}

impl FromRow for Held {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.int("id")?,
            relation: row.text("relation")?,
        })
    }
}

/// The strongest relation the principal holds on each document.
async fn held(reads: &impl Reads, subjects: &SubjectSet) -> Outcome<Vec<(i64, Relation)>> {
    let mut best: Vec<(i64, Relation)> = Vec::new();
    for row in reads
        .query::<Held>(HELD_SQL, bind_keys(subjects))
        .await?
        .into_iter()
    {
        let Some(relation) = Relation::parse(&row.relation) else {
            continue;
        };
        match best.iter_mut().find(|(id, _)| *id == row.id) {
            Some(entry) if relation.covers(entry.1) => entry.1 = relation,
            Some(_) => {}
            None => best.push((row.id, relation)),
        }
    }
    Ok(best)
}

fn bind_keys(subjects: &SubjectSet) -> Vec<rn_kernel::store::Value> {
    vec![rn_kernel::store::Value::from(subject_keys(subjects))]
}

/// Whether one of the principal's own parties owns this row.
fn owned_by(subjects: &SubjectSet, owner: i64) -> bool {
    subjects.person.is_some_and(|person| person.get() == owner)
        || subjects.organizations.iter().any(|org| org.get() == owner)
}

fn summary(
    row: &DocumentSummary,
    subjects: &SubjectSet,
    held: &[(i64, Relation)],
    key: &IdKey,
) -> serde_json::Value {
    let mine = owned_by(subjects, row.owner_id);
    let relation = if mine {
        Relation::Owner
    } else {
        held.iter()
            .find(|(id, _)| *id == row.id.get())
            .map_or(Relation::Viewer, |(_, relation)| *relation)
    };
    json!({
        "public_id": row.id.public(key),
        "title": row.title,
        "status": row.status,
        "created_at": row.created_at,
        "draft_rev": row.draft_rev,
        "published_rev": row.published_rev,
        "unpublished": row.published_rev != Some(row.draft_rev),
        "mine": mine,
        "relation": relation.as_str(),
        "may_edit": relation.covers(Relation::Editor),
        "owner": match PartyKind::parse(&row.owner_kind) {
            Some(kind) => json!({
                "public_id": public_of(kind, row.owner_id, key),
                "kind": kind.as_str(),
                "display": row.owner_display,
            }),
            None => json!({ "kind": row.owner_kind, "display": row.owner_display }),
        },
    })
}

/// Every document this principal can reach.
pub(super) async fn documents(
    reads: &impl Reads,
    subjects: &SubjectSet,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    let rows = reads
        .query::<DocumentSummary>(
            DOCUMENTS_SQL,
            rn_kernel::bind![subject_keys(subjects), owner_ids(subjects), PAGE],
        )
        .await?;
    let held = held(reads, subjects).await?;
    Ok(rows
        .into_iter()
        .map(|row| Row {
            key: row.id.public(key).as_str().to_owned(),
            value: summary(&row, subjects, &held, key),
        })
        .collect())
}

/// One document, in full, or a decline.
pub(super) async fn document(
    reads: &impl Reads,
    subjects: &SubjectSet,
    params: &Params,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    let Some(raw) = params.id.as_deref() else {
        return decline();
    };
    let Ok(id) = raw.parse::<PublicId>() else {
        return decline();
    };
    let Ok(resource) = ids::decode::<Resource>(key, &id) else {
        return decline();
    };
    if !check(
        reads,
        subjects,
        Relation::Viewer,
        Object::document(resource),
    )
    .await?
    {
        return decline();
    }
    let Some(full) = reads
        .query_opt::<Full>(DOCUMENT_SQL, rn_kernel::bind![resource])
        .await?
    else {
        return decline();
    };
    let held = held(reads, subjects).await?;
    let mut value = summary(&full.summary, subjects, &held, key);
    if let Some(fields) = value.as_object_mut() {
        fields.insert("body".to_owned(), json!(full.body));
    }
    Ok(vec![Row {
        key: full.summary.id.public(key).as_str().to_owned(),
        value,
    }])
}

/// A document and its words. The summary is read first, because a cursor
/// consumes a column as it converts it and both halves name `title`.
struct Full {
    summary: DocumentSummary,
    body: String,
}

impl FromRow for Full {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            summary: DocumentSummary::from_row(row)?,
            body: row.text("body")?,
        })
    }
}

struct Share {
    object_id: i64,
    relation: String,
    at: Timestamp,
    party_kind: String,
    display: String,
    party_id: Id<Person>,
}

impl FromRow for Share {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            object_id: row.int("object_id")?,
            relation: row.text("relation")?,
            at: row.int("at")?,
            party_kind: row.text("party_kind")?,
            display: row.text("display")?,
            party_id: row.id("party_id")?,
        })
    }
}

/// Every grant on every document this principal can reach.
pub(super) async fn shares(
    reads: &impl Reads,
    subjects: &SubjectSet,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    let visible = reads
        .query::<DocumentSummary>(
            DOCUMENTS_SQL,
            rn_kernel::bind![subject_keys(subjects), owner_ids(subjects), PAGE],
        )
        .await?;
    let ids: Vec<i64> = visible.iter().map(|row| row.id.get()).collect();
    let list = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_owned());

    Ok(reads
        .query::<Share>(SHARES_SQL, rn_kernel::bind![list])
        .await?
        .into_iter()
        .filter_map(|row| {
            let kind = PartyKind::parse(&row.party_kind)?;
            let subject = public_of(kind, row.party_id.get(), key);
            let document = Id::<Resource>::new(row.object_id).public(key);
            Some(Row {
                key: format!(
                    "{}:{}:{}",
                    document.as_str(),
                    subject.as_str(),
                    row.relation
                ),
                value: json!({
                    "document": document,
                    "subject": subject,
                    "kind": kind.as_str(),
                    "display": row.display,
                    "relation": row.relation,
                    "at": row.at,
                }),
            })
        })
        .collect())
}
