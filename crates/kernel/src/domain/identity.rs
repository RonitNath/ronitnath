//! `identity` — a registration, not a human.
//!
//! Everything a product stores points at an identity. That is the whole
//! reason merging humans is cheap: resolving two identities onto one person
//! writes `person_link` rows and rewrites nothing else.

use super::Vocabulary;
use crate::Timestamp;
use crate::ids::{Id, Identity, Person};
use crate::store::{Cursor, FromRow, RowError};

/// Whether an identity still authenticates.
///
/// There is no `pending`: an identity that could not sign in until an email
/// came back would lock every new registration out of its own session, and the
/// verification state lives on the factor, where it belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdentityStatus {
    /// Signs in.
    Active,
    /// Does not. Its sessions are deleted when it lands here.
    Disabled,
}

impl Vocabulary for IdentityStatus {
    const ALL: &'static [Self] = &[Self::Active, Self::Disabled];
    const COLUMN: (&'static str, &'static str) = ("identity", "status");

    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }
}

/// An `identity` row.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentityRow {
    /// The rowid.
    pub id: Id<Identity>,
    /// Where the registration happened.
    pub source: String,
    /// The human it has been resolved to, if any.
    pub person_id: Option<Id<Person>>,
    /// The zone its rows live in.
    pub home_zone: String,
    /// Whether it authenticates.
    pub status: IdentityStatus,
    /// When it registered, unix seconds.
    pub created_at: Timestamp,
}

impl IdentityRow {
    /// The columns this row reads.
    pub const COLUMNS: &'static str = "id, source, person_id, home_zone, status, created_at";
}

impl FromRow for IdentityRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let status = row.text("status")?;
        Ok(Self {
            id: row.id("id")?,
            source: row.text("source")?,
            person_id: row.id_opt("person_id")?,
            home_zone: row.text("home_zone")?,
            status: IdentityStatus::parse(&status).ok_or(RowError::Column {
                column: "status".to_owned(),
                wanted: "IdentityStatus",
            })?,
            created_at: row.int("created_at")?,
        })
    }
}
