//! The one relation store: `object #relation @subject`.
//!
//! Every "may X do Y to Z" in this system is answered from one table. A
//! document shared with a person, an invitation link that carries a role into
//! a group, a photo a whole group can see, a person's contact details scoped
//! to one group, platform administration — all of them are rows here, and
//! leaking across a boundary would need a row somebody wrote.
//!
//! ## How a subject is addressed
//!
//! A relation row carries `(subject_kind, subject_id)`, and migration 2 adds
//! the generated column `subject_key = subject_kind || ':' || subject_id`
//! together with an index on `(subject_key, object_kind, object_id, relation)`.
//! That is what makes [`check`](check::check) one indexed query: a principal's
//! expanded subject set is a handful of keys, they go in as a JSON array, and
//! `json_each` joins them against that index. Without the generated column the
//! same query would be one `OR` branch per subject kind, and SQLite would pick
//! at most one of them for an index.
//!
//! The subject set is *small and bounded by real membership*, while the number
//! of grants on an object is chosen by whoever shares it. Keying the hot query
//! on the subject side is therefore the shape that stays fast under the
//! adversarial case a stranger can actually produce: an object shared ten
//! thousand times costs a `check` nothing at all.
//!
//! ## Ownership is a row too
//!
//! `resource.owner_party_id` is authoritative — the schema's trigger is what
//! makes "a group never owns" unrepresentable — but `check` does not read it.
//! `CreateDocument` and `Transfer` also write `<kind>:<id> #owner @<owner>`
//! into this table, inside the same transaction, so ownership is answered by
//! the same single query as everything else. The two are written together and
//! a proptest asserts they never disagree.

mod check;
mod kinds;
mod refs;
#[cfg(test)]
mod tests;
mod vocabulary;

pub use check::{
    CHECK_SQL, CONTACTS_SQL, LIST_VISIBLE_SQL, Page, check, check_principal, list_visible,
    visible_contacts,
};
pub use kinds::PARTY_SUBJECTS;
pub use refs::{Grant, Object, Subject};
// `Vocabulary` here is the registry; `domain::Vocabulary` is the
// schema-agreement trait. Different jobs, and the paths keep them apart.
pub use vocabulary::{Admits, Kind, Relation, SubjectKind, Vocabulary};

use crate::domain::Vocabulary as SchemaVocabulary;
use crate::error::Outcome;
use crate::ids::{Id, Identity};
use crate::principal::SubjectSet;
use crate::store::{Reads, Stmt, Value};
use crate::{Timestamp, bind};

/// The columns a [`Grant`] reads.
const GRANT_COLUMNS: &str = "object_kind, object_id, relation, subject_kind, subject_id, at";

/// Insert a grant. `INSERT OR IGNORE`, because granting what is already
/// granted is the same world, not a failure — and the UNIQUE index is what
/// makes it so.
///
/// Public, so a command whose subject id does not exist until its own batch
/// runs — an invitation link — can bind a statement reference where
/// [`grant_stmt`] wants a value.
pub const GRANT_SQL: &str = "INSERT OR IGNORE INTO relation \
                         (object_kind, object_id, relation, subject_kind, subject_id, \
                          granted_by, at) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7)";

/// Delete exactly one grant — the row that was written, and no other path the
/// subject may also hold.
pub const REVOKE_SQL: &str = "DELETE FROM relation \
                          WHERE object_kind = $1 AND object_id = $2 AND relation = $3 \
                            AND subject_kind = $4 AND subject_id = $5";

const BY_OBJECT_SQL: &str = "SELECT object_kind, object_id, relation, subject_kind, subject_id, at \
                             FROM relation WHERE object_kind = $1 AND object_id = $2 \
                             ORDER BY id";

const BY_SUBJECT_SQL: &str = "SELECT object_kind, object_id, relation, subject_kind, subject_id, at \
     FROM relation WHERE subject_key = $1 ORDER BY id";

/// The statement that writes one grant, for a command to put in its batch.
///
/// A grant is almost never written on its own: it belongs to the transaction
/// of the command that decided it. This is the statement; [`grant`] is the
/// out-of-band form, for seeding and for tests.
pub fn grant_stmt(
    object: Object,
    relation: Relation,
    subject: Subject,
    granted_by: Option<Id<Identity>>,
    at: Timestamp,
) -> Stmt {
    Stmt::any(
        GRANT_SQL,
        vec![
            Value::from(object.kind),
            Value::from(object.id),
            Value::from(relation.as_str()),
            Value::from(subject.kind.as_str()),
            Value::from(subject.id),
            Value::from(granted_by),
            Value::from(at),
        ],
    )
}

/// The statement that removes one grant.
pub fn revoke_stmt(object: Object, relation: Relation, subject: Subject) -> Stmt {
    Stmt::any(
        REVOKE_SQL,
        vec![
            Value::from(object.kind),
            Value::from(object.id),
            Value::from(relation.as_str()),
            Value::from(subject.kind.as_str()),
            Value::from(subject.id),
        ],
    )
}

/// Write a grant outside a command's transaction.
///
/// For seeding a deployment's first platform operator and for tests. Product
/// code shares through the `Share` command, which authorises first.
pub async fn grant<S: crate::store::Sql>(
    store: &crate::store::Store<S>,
    object: Object,
    relation: Relation,
    subject: Subject,
) -> Outcome<()> {
    let at = store.clock().now();
    let stmt = grant_stmt(object, relation, subject, None, at);
    store.execute(GRANT_SQL, stmt.params).await?;
    Ok(())
}

/// Remove a grant outside a command's transaction.
pub async fn revoke<S: crate::store::Sql>(
    store: &crate::store::Store<S>,
    object: Object,
    relation: Relation,
    subject: Subject,
) -> Outcome<()> {
    let stmt = revoke_stmt(object, relation, subject);
    store.execute(REVOKE_SQL, stmt.params).await?;
    Ok(())
}

/// Every grant on one object — what a sharing panel renders.
pub async fn list_for_object(store: &impl Reads, object: Object) -> Outcome<Vec<Grant>> {
    Ok(store
        .query::<Grant>(BY_OBJECT_SQL, bind![object.kind, object.id])
        .await?)
}

/// Every grant one subject holds.
pub async fn list_for_subject(store: &impl Reads, subject: Subject) -> Outcome<Vec<Grant>> {
    Ok(store
        .query::<Grant>(BY_SUBJECT_SQL, bind![subject.key()])
        .await?)
}

/// The columns a grant listing returns, exposed so a test can assert the
/// query and its mapping have not drifted.
pub const GRANT_COLUMN_LIST: &str = GRANT_COLUMNS;

/// Every subject a principal's expansion stands for, as the JSON array
/// `check()` joins against.
///
/// `public` is always in it and is not stored on the [`SubjectSet`]: it is
/// what "everyone" means, and a principal that did not have it would be
/// nobody at all.
pub fn subject_keys(set: &SubjectSet) -> String {
    let mut keys = vec![Subject::public().key()];
    if set.authenticated {
        keys.push(Subject::authenticated().key());
    }
    if let Some(identity) = set.identity {
        keys.push(Subject::of(SubjectKind::Identity, identity).key());
    }
    if let Some(person) = set.person {
        keys.push(Subject::person(person).key());
    }
    if let Some(link) = set.link {
        keys.push(Subject::link(link).key());
    }
    keys.extend(
        set.organizations
            .iter()
            .map(|o| Subject::organization(*o).key()),
    );
    keys.extend(set.groups.iter().map(|g| Subject::group(*g).key()));
    serde_json::to_string(&keys).unwrap_or_else(|_| "[]".to_owned())
}

/// The party rows a principal stands for, as a JSON array of ids.
///
/// This is the membership half of `check()` and the ownership half of
/// `list_visible`: both ask "is one of *these parties* on that row", which is
/// an integer question and not a subject-key one.
pub fn party_ids(set: &SubjectSet) -> String {
    let mut ids: Vec<i64> = Vec::with_capacity(1 + set.organizations.len() + set.groups.len());
    if let Some(person) = set.person {
        ids.push(person.get());
    }
    ids.extend(set.organizations.iter().map(|o| o.get()));
    ids.extend(set.groups.iter().map(|g| g.get()));
    serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_owned())
}

/// The parties a principal may name as an owner: the person and the
/// organizations, never the groups.
pub fn owner_ids(set: &SubjectSet) -> String {
    let mut ids: Vec<i64> = Vec::with_capacity(1 + set.organizations.len());
    if let Some(person) = set.person {
        ids.push(person.get());
    }
    ids.extend(set.organizations.iter().map(|o| o.get()));
    serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_owned())
}

/// An empty JSON array — what a query is handed for a branch that does not
/// apply, so the branch costs nothing rather than being guarded by a flag.
pub const NO_IDS: &str = "[]";
