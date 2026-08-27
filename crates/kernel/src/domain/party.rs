//! `party` — anything that can own, be granted, or belong.

use super::Vocabulary;
use crate::Timestamp;
use crate::ids::{Id, Person};
use crate::store::{Cursor, FromRow, RowError};

/// What sort of party a row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartyKind {
    /// The human. Global, one per human eventually.
    Person,
    /// A tenant and ownership boundary.
    Organization,
    /// A set of parties. Granted things, never owning them.
    Group,
    /// A machine principal. No route accepts one in this cut.
    Service,
}

impl Vocabulary for PartyKind {
    const ALL: &'static [Self] = &[Self::Person, Self::Organization, Self::Group, Self::Service];
    const COLUMN: (&'static str, &'static str) = ("party", "kind");

    fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organization => "organization",
            Self::Group => "group",
            Self::Service => "service",
        }
    }
}

impl From<PartyKind> for rn_api::whoami::PartyKind {
    fn from(kind: PartyKind) -> Self {
        match kind {
            PartyKind::Person => Self::Person,
            PartyKind::Organization => Self::Organization,
            PartyKind::Group => Self::Group,
            PartyKind::Service => Self::Service,
        }
    }
}

/// A party's status — a projection of the commands that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartyStatus {
    /// Authenticates, is a usable subject.
    Active,
    /// Disabled by an operator or by the person. Reversible.
    Disabled,
    /// Absorbed by a merge. The row stays so old ids keep resolving.
    Merged,
}

impl Vocabulary for PartyStatus {
    const ALL: &'static [Self] = &[Self::Active, Self::Disabled, Self::Merged];
    const COLUMN: (&'static str, &'static str) = ("party", "status");

    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Merged => "merged",
        }
    }
}

/// A `party` row. The id is typed as a person's because that is the only kind
/// this leg reads back; K2 reads organizations and groups the same way.
#[derive(Debug, Clone, PartialEq)]
pub struct PartyRow {
    /// The rowid.
    pub id: Id<Person>,
    /// Which sort of party.
    pub kind: PartyKind,
    /// How a UI names it.
    pub display_name: String,
    /// Its status.
    pub status: PartyStatus,
    /// When it was created, unix seconds.
    pub created_at: Timestamp,
}

impl PartyRow {
    /// The columns this row reads, so a query and its mapping cannot drift.
    pub const COLUMNS: &'static str = "id, kind, display_name, status, created_at";
}

impl FromRow for PartyRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let kind = row.text("kind")?;
        let status = row.text("status")?;
        Ok(Self {
            id: row.id("id")?,
            kind: PartyKind::parse(&kind).ok_or(RowError::Column {
                column: "kind".to_owned(),
                wanted: "PartyKind",
            })?,
            display_name: row.text("display_name")?,
            status: PartyStatus::parse(&status).ok_or(RowError::Column {
                column: "status".to_owned(),
                wanted: "PartyStatus",
            })?,
            created_at: row.int("created_at")?,
        })
    }
}
