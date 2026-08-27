//! How an object and a subject are named, and the row that joins them.
//!
//! An [`Object`] is a registered kind plus the row that kind addresses: the
//! `resource` row for a resource-backed kind, the `party` row for an
//! organization, a group or a person, and `0` for `platform`, which is a
//! singleton with no row at all. A [`Subject`] is who holds the relation —
//! including the two that name no row, `public` and `authenticated`, which is
//! why the schema gives them id `0` rather than NULL.

use super::vocabulary::{Relation, SubjectKind};
use crate::Timestamp;
use crate::domain::Vocabulary as SchemaVocabulary;
use crate::ids::{Group, Id, Link, Organization, Person, Resource, Table};
use crate::store::{Cursor, FromRow, RowError};

/// What a relation is granted *on*.
///
/// `kind` is a registered kind name and `id` is the row that kind addresses:
/// the `resource` row for a resource-backed kind, the `party` row for an
/// organization, a group or a person, and `0` for `platform`, which is a
/// singleton with no row at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Object {
    /// The registered kind name.
    pub kind: &'static str,
    /// The row it addresses.
    pub id: i64,
}

impl Object {
    /// A document, addressed by its resource row.
    pub const fn document(id: Id<Resource>) -> Self {
        Self {
            kind: "document",
            id: id.get(),
        }
    }

    /// A resource of some registered kind.
    pub const fn resource(kind: &'static str, id: Id<Resource>) -> Self {
        Self { kind, id: id.get() }
    }

    /// An organization, addressed by its party row.
    pub const fn organization(id: Id<Organization>) -> Self {
        Self {
            kind: "organization",
            id: id.get(),
        }
    }

    /// A group, addressed by its party row.
    pub const fn group(id: Id<Group>) -> Self {
        Self {
            kind: "group",
            id: id.get(),
        }
    }

    /// A person, addressed by their party row.
    pub const fn person(id: Id<Person>) -> Self {
        Self {
            kind: "person",
            id: id.get(),
        }
    }

    /// The platform itself — `platform:*`, the singleton every operator
    /// relation hangs off.
    pub const fn platform() -> Self {
        Self {
            kind: "platform",
            id: 0,
        }
    }
}

/// Who a relation is granted *to*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subject {
    /// What sort of subject.
    pub kind: SubjectKind,
    /// Its row, or `0` for `public` and `authenticated`.
    pub id: i64,
}

impl Subject {
    /// A subject naming a row.
    pub const fn of<T: Table>(kind: SubjectKind, id: Id<T>) -> Self {
        Self { kind, id: id.get() }
    }

    /// A person.
    pub const fn person(id: Id<Person>) -> Self {
        Self::of(SubjectKind::Person, id)
    }

    /// An organization.
    pub const fn organization(id: Id<Organization>) -> Self {
        Self::of(SubjectKind::Organization, id)
    }

    /// A group.
    pub const fn group(id: Id<Group>) -> Self {
        Self::of(SubjectKind::Group, id)
    }

    /// An invitation link: whoever holds the token.
    pub const fn link(id: Id<Link>) -> Self {
        Self::of(SubjectKind::Link, id)
    }

    /// Everyone.
    pub const fn public() -> Self {
        Self {
            kind: SubjectKind::Public,
            id: 0,
        }
    }

    /// Anyone signed in.
    pub const fn authenticated() -> Self {
        Self {
            kind: SubjectKind::Authenticated,
            id: 0,
        }
    }

    /// The value of the generated `subject_key` column for this subject.
    pub fn key(self) -> String {
        format!("{}:{}", self.kind.as_str(), self.id)
    }
}

/// One relation row, as a reader sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    /// What it is on.
    pub object_kind: String,
    /// Which row.
    pub object_id: i64,
    /// What is granted.
    pub relation: Relation,
    /// To whom.
    pub subject: Subject,
    /// When.
    pub at: Timestamp,
}

impl FromRow for Grant {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let relation = row.text("relation")?;
        let subject_kind = row.text("subject_kind")?;
        Ok(Self {
            object_kind: row.text("object_kind")?,
            object_id: row.int("object_id")?,
            relation: Relation::parse(&relation).ok_or(RowError::Column {
                column: "relation".to_owned(),
                wanted: "Relation",
            })?,
            subject: Subject {
                kind: SubjectKind::parse(&subject_kind).ok_or(RowError::Column {
                    column: "subject_kind".to_owned(),
                    wanted: "SubjectKind",
                })?,
                id: row.int("subject_id")?,
            },
            at: row.int("at")?,
        })
    }
}
