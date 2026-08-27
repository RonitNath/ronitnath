//! The resource registry: every ownable thing has a row here.
//!
//! `resource` carries the four facts that are true of anything ownable — its
//! kind, who owns it, which zone it lives in, and its status — and the kind
//! tables hang off it 1:1. A document is a `resource` row plus a `document`
//! row; an organization is a `resource` row plus a `party_resource` row
//! pointing at the `party` that memberships and relations address.
//!
//! Two rules are carried by the schema rather than by code that remembers
//! them: a group never appears as `owner_party_id` (migration 1's trigger),
//! and `owner_party_id` moves only through the `Transfer` command (nothing
//! else writes that column — a proptest asserts it over random command
//! sequences).
//!
//! ## Statuses
//!
//! `draft | published`, and they are projections of the commands that produced
//! them. A document starts `draft` and `PublishDocument` moves it. The two
//! party-backed kinds are created `published`: an organization is live the
//! moment it exists, and a draft organization is not a thing any command can
//! make.
//!
//! Migration 1's CHECK also admits `deleted`, and no command writes it — there
//! is no delete in this cut. The value cannot leave the schema before the next
//! fresh formation, because migrations are hashed, so it is declared
//! unproduced ([`crate::domain::Vocabulary::ADMITTED_UNPRODUCED`]) instead of
//! carried as a variant nothing reaches. The queries that read a list keep
//! their `status <> 'deleted'` filter: the filter costs nothing, and it is the
//! statement rung 7's delete will already be written against.

#[cfg(test)]
mod tests;

use crate::domain::Vocabulary;
use crate::error::Outcome;
use crate::ids::{Group, Id, Organization, Person, Resource};
use crate::store::{Cursor, FromRow, Reads, RowError};
use crate::{Timestamp, bind};

/// The registered kind names, as they appear in `resource.kind` and in
/// `relation.object_kind`.
pub mod kinds {
    /// The first product kind.
    pub const DOCUMENT: &str = "document";
    /// A tenant and ownership boundary.
    pub const ORGANIZATION: &str = "organization";
    /// A set of parties.
    pub const GROUP: &str = "group";
}

/// A resource's status. See the module docs for the third value the schema
/// admits and no command writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceStatus {
    /// Made, not yet published.
    Draft,
    /// Live.
    Published,
}

impl Vocabulary for ResourceStatus {
    const ALL: &'static [Self] = &[Self::Draft, Self::Published];
    const ADMITTED_UNPRODUCED: &'static [&'static str] = &["deleted"];
    const COLUMN: (&'static str, &'static str) = ("resource", "status");

    fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
        }
    }
}

/// A `resource` row.
///
/// `owner_party_id` is typed as a person's id the way `PartyRow::id` is: a
/// party id is a party id inside the process, and the kinds are kept apart
/// where it matters, which is the public id a caller sends.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceRow {
    /// The rowid.
    pub id: Id<Resource>,
    /// Which registered kind.
    pub kind: String,
    /// The person or organization that owns it.
    pub owner_party_id: Id<Person>,
    /// The zone the row lives in.
    pub home_zone: String,
    /// Its status.
    pub status: ResourceStatus,
    /// When it was created, unix seconds.
    pub created_at: Timestamp,
}

impl ResourceRow {
    /// The columns this row reads, so a query and its mapping cannot drift.
    pub const COLUMNS: &'static str = "id, kind, owner_party_id, home_zone, status, created_at";

    /// Whether one of these party ids owns this resource.
    pub fn owned_by_any(&self, parties: &[Id<Person>]) -> bool {
        parties.contains(&self.owner_party_id)
    }
}

impl FromRow for ResourceRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let status = row.text("status")?;
        Ok(Self {
            id: row.id("id")?,
            kind: row.text("kind")?,
            owner_party_id: row.id("owner_party_id")?,
            home_zone: row.text("home_zone")?,
            status: ResourceStatus::parse(&status).ok_or(RowError::Column {
                column: "status".to_owned(),
                wanted: "ResourceStatus",
            })?,
            created_at: row.int("created_at")?,
        })
    }
}

/// One resource by id.
pub const BY_ID_SQL: &str = "SELECT id, kind, owner_party_id, home_zone, status, created_at \
                             FROM resource WHERE id = $1";

/// The resource row that owns a party — an organization's or a group's.
pub const BY_PARTY_SQL: &str = "SELECT r.id, r.kind, r.owner_party_id, r.home_zone, r.status, r.created_at \
     FROM party_resource pr JOIN resource r ON r.id = pr.resource_id \
     WHERE pr.party_id = $1";

/// Read one resource.
pub async fn load(store: &impl Reads, id: Id<Resource>) -> Outcome<Option<ResourceRow>> {
    Ok(store.query_opt::<ResourceRow>(BY_ID_SQL, bind![id]).await?)
}

/// Read the resource row behind a party — the ownership half of an
/// organization or a group.
pub async fn of_party(store: &impl Reads, party: Id<Person>) -> Outcome<Option<ResourceRow>> {
    Ok(store
        .query_opt::<ResourceRow>(BY_PARTY_SQL, bind![party])
        .await?)
}

/// [`of_party`], for an organization.
pub async fn of_organization(
    store: &impl Reads,
    org: Id<Organization>,
) -> Outcome<Option<ResourceRow>> {
    of_party(store, Id::new(org.get())).await
}

/// [`of_party`], for a group.
pub async fn of_group(store: &impl Reads, group: Id<Group>) -> Outcome<Option<ResourceRow>> {
    of_party(store, Id::new(group.get())).await
}

/// Insert a resource row. Every kind's creation statement is this one.
pub const INSERT_SQL: &str = "INSERT INTO resource (kind, owner_party_id, home_zone, status, \
                              created_at) VALUES ($1, $2, $3, $4, $5) RETURNING id";

/// Join a party to the resource row that owns it.
pub const INSERT_PARTY_RESOURCE_SQL: &str =
    "INSERT INTO party_resource (resource_id, party_id) VALUES ($1, $2)";

/// Move ownership. The guard is the *current* owner, so a transfer that raced
/// another transfer writes nothing rather than overwriting the winner.
pub const TRANSFER_SQL: &str =
    "UPDATE resource SET owner_party_id = $1 WHERE id = $2 AND owner_party_id = $3";
