//! `sub`, pairwise by sector.
//!
//! An RP's whole notion of who somebody is rests on this string, so three
//! things are true of it and each one is load-bearing.
//!
//! **It is never an id.** A public id is a reversible encryption of a rowid
//! under this deployment's key; a `sub` is 256 random bits that name nothing.
//! Handing an RP an id would make the identifier it stores the identifier
//! every other endpoint addresses rows by.
//!
//! **It is pairwise by *sector*, and the sector is the client's owner.** Two
//! applications of one organization recognise the same person; two
//! organizations cannot correlate their users at all, however many clients
//! each registers. For operator-registered clients the sector is the
//! deployment's platform party — `platform:*` — which is why those clients do
//! see one another's subjects: they are one party's.
//!
//! **It is minted once and never rotated.** Rotating it would silently make
//! one person two at every RP that had ever seen them. After a merge the
//! absorbed person's row stays and both subs still resolve, because a `sub`
//! names a person and a person resolves through `person_alias`.

use crate::Timestamp;
use crate::domain::Token;
use crate::error::Outcome;
use crate::ids::{Id, Person};
use crate::store::{Cursor, FromRow, Reads, RowError};
use crate::{bind, store::Stmt, store::Value};

/// The party the platform's own clients are pairwise under.
///
/// `platform:*` is a singleton with no `party` row — the relation store
/// addresses it as object id `0` — so the sector column carries `0` too, and
/// the foreign key is satisfied by nothing pointing at it. A row that names a
/// real organization names that organization's `party` row.
pub const PLATFORM_SECTOR: i64 = 0;

/// One `oidc_subject` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRow {
    /// The sector — the client's owner, or [`PLATFORM_SECTOR`].
    pub sector_party_id: i64,
    /// The person it names.
    pub person_id: Id<Person>,
    /// The string an RP stores.
    pub sub: String,
}

impl FromRow for SubjectRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            sector_party_id: row.int("sector_party_id")?,
            person_id: row.id("person_id")?,
            sub: row.text("sub")?,
        })
    }
}

/// The pair lookup: one seek on `oidc_subject_pair_idx`.
pub const BY_PAIR_SQL: &str = "SELECT sector_party_id, person_id, sub FROM oidc_subject \
     WHERE sector_party_id = $1 AND person_id = $2";

/// The reverse: which person a `sub` names, resolved through a merge.
///
/// `person_alias` is the join that makes an absorbed person's sub keep
/// working: the row still says the absorbed person, and this says who that
/// person is now.
pub const BY_SUB_SQL: &str = "SELECT s.sector_party_id, \
     coalesce(a.person_id, s.person_id) AS person_id, s.sub \
     FROM oidc_subject s LEFT JOIN person_alias a ON a.old_person_id = s.person_id \
     WHERE s.sub = $1";

/// Mint one. `INSERT OR IGNORE`, because two requests racing to mint the same
/// person's first subject at one sector must produce one row, and the UNIQUE
/// index is what makes it so.
pub const INSERT_SQL: &str = "INSERT OR IGNORE INTO oidc_subject \
     (sector_party_id, person_id, sub, created_at) VALUES ($1, $2, $3, $4)";

/// The subject a person already has at a sector.
pub async fn existing(
    store: &impl Reads,
    sector: i64,
    person: Id<Person>,
) -> Outcome<Option<SubjectRow>> {
    Ok(store
        .query_opt::<SubjectRow>(BY_PAIR_SQL, bind![sector, person])
        .await?)
}

/// The person a `sub` names, following a merge.
pub async fn person_of(store: &impl Reads, sub: &str) -> Outcome<Option<Id<Person>>> {
    Ok(store
        .query_opt::<SubjectRow>(BY_SUB_SQL, bind![sub])
        .await?
        .map(|row| row.person_id))
}

/// A fresh subject string: 256 random bits, base64url, 43 ASCII characters —
/// well inside the 255 the specification allows and outside anything a
/// deployment could enumerate.
#[must_use]
pub fn mint() -> String {
    Token::mint().expose().to_owned()
}

/// The statement that writes one, for a command's batch.
#[must_use]
pub fn insert_stmt(sector: i64, person: Id<Person>, sub: &str, at: Timestamp) -> Stmt {
    Stmt::any(
        INSERT_SQL,
        vec![
            Value::from(sector),
            Value::from(person),
            Value::from(sub),
            Value::from(at),
        ],
    )
}

/// The subject a person will be known by at a sector, minting one if they
/// have none — and the statement to write it, if it is new.
///
/// Two returns rather than one, because minting is a write and this is called
/// from a command's *plan*: the string is needed to build the rest of the
/// batch, and the row it comes from goes in the same batch.
pub async fn resolve(
    store: &impl Reads,
    sector: i64,
    person: Id<Person>,
    at: Timestamp,
) -> Outcome<(String, Option<Stmt>)> {
    if let Some(row) = existing(store, sector, person).await? {
        return Ok((row.sub, None));
    }
    let sub = mint();
    let stmt = insert_stmt(sector, person, &sub, at);
    Ok((sub, Some(stmt)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subject_is_random_and_short_enough_for_the_specification() {
        let a = mint();
        let b = mint();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(a.is_ascii() && a.len() <= 255);
    }
}
