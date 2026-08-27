//! Groups: a set of parties, granted things and owning none.
//!
//! A group has the same three rows an organization has (`crate::org` spells
//! the shape out) with `kind='group'`. What differs is what it is *for*:
//!
//! * a group belongs to an organization, or to one person — a contact list, a
//!   cohort, the audience of an invitation. `resource.owner_party_id` says
//!   which, and it is the only place that answers it.
//! * a group is never an owner. The trigger in migration 1 refuses one as
//!   `owner_party_id` on insert and on update, so a bug in a command produces
//!   a failed transaction rather than a group that owns a document.
//! * a group is the unit contact scoping is written against:
//!   `person:P #contact @group:X` is how one person's details become visible
//!   inside a group and nowhere else, and
//!   [`visible_contacts`](crate::relation::visible_contacts) is that rule as a
//!   query.

use crate::domain::PartyRow;
use crate::error::Outcome;
use crate::ids::{Group, Id, Organization, Person};
use crate::org::{self, Member, MemberRole};
use crate::resource::{self, ResourceRow};
use crate::store::Reads;
use crate::{bind, store::Cursor, store::FromRow, store::RowError};

/// A group, both halves: the party that is granted and belongs, and the
/// resource row that says who owns it.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupRow {
    /// The party half.
    pub party: PartyRow,
    /// The resource half.
    pub resource: ResourceRow,
    /// What sort of party owns it. Not a column — it is the owner party's
    /// kind, read in the same join, because "is this a personal group" is a
    /// question every caller asks and none should join for itself.
    pub owner_kind: OwnerKind,
}

impl GroupRow {
    /// The group's party id — what memberships and relations address.
    pub const fn id(&self) -> Id<Group> {
        Id::new(self.party.id.get())
    }

    /// The organization this group belongs to, if it belongs to one rather
    /// than to a person.
    pub fn organization(&self) -> Option<Id<Organization>> {
        (self.owner_kind == OwnerKind::Organization)
            .then(|| Id::new(self.resource.owner_party_id.get()))
    }

    /// Whether this is somebody's personal group.
    pub const fn is_personal(&self) -> bool {
        matches!(self.owner_kind, OwnerKind::Person)
    }
}

/// Whether a group's owner is a person or an organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerKind {
    /// Somebody's own group.
    Person,
    /// An organization's group.
    Organization,
}

/// One group, both halves and the kind of its owner.
pub const LOAD_SQL: &str = "SELECT p.id, p.kind, p.display_name, p.status, p.created_at, \
     r.id AS resource_id, r.kind AS resource_kind, r.owner_party_id, r.home_zone, \
     r.status AS resource_status, r.created_at AS resource_created_at, \
     owner.kind AS owner_kind \
     FROM party p \
     JOIN party_resource pr ON pr.party_id = p.id \
     JOIN resource r ON r.id = pr.resource_id \
     JOIN party owner ON owner.id = r.owner_party_id \
     WHERE p.id = $1 AND p.kind = $2";

struct Loaded {
    group: GroupRow,
}

impl FromRow for Loaded {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let party = PartyRow::from_row(row)?;
        let status = row.text("resource_status")?;
        let owner_kind = row.text("owner_kind")?;
        let resource = ResourceRow {
            id: row.id("resource_id")?,
            kind: row.text("resource_kind")?,
            owner_party_id: row.id("owner_party_id")?,
            home_zone: row.text("home_zone")?,
            status: <crate::resource::ResourceStatus as crate::domain::Vocabulary>::parse(&status)
                .ok_or(RowError::Column {
                    column: "resource_status".to_owned(),
                    wanted: "ResourceStatus",
                })?,
            created_at: row.int("resource_created_at")?,
        };
        Ok(Self {
            group: GroupRow {
                party,
                resource,
                owner_kind: match owner_kind.as_str() {
                    "organization" => OwnerKind::Organization,
                    _ => OwnerKind::Person,
                },
            },
        })
    }
}

/// Read one group.
pub async fn load(store: &impl Reads, group: Id<Group>) -> Outcome<Option<GroupRow>> {
    Ok(store
        .query_opt::<Loaded>(LOAD_SQL, bind![group, crate::resource::kinds::GROUP])
        .await?
        .map(|row| row.group))
}

/// Read one organization, which has the same shape.
pub async fn load_organization(
    store: &impl Reads,
    org: Id<Organization>,
) -> Outcome<Option<GroupRow>> {
    Ok(store
        .query_opt::<Loaded>(
            LOAD_SQL,
            bind![
                Id::<Person>::new(org.get()),
                crate::resource::kinds::ORGANIZATION
            ],
        )
        .await?
        .map(|row| row.group))
}

/// Everyone in a group.
pub async fn members(store: &impl Reads, group: Id<Group>) -> Outcome<Vec<Member>> {
    org::members(store, Id::new(group.get())).await
}

/// What role a party holds in a group.
pub async fn role_of(
    store: &impl Reads,
    group: Id<Group>,
    party: Id<Person>,
) -> Outcome<Option<MemberRole>> {
    org::role_of(store, Id::new(group.get()), party).await
}

/// The resource row a group is owned through.
pub async fn owning_resource(store: &impl Reads, group: Id<Group>) -> Outcome<Option<ResourceRow>> {
    resource::of_group(store, group).await
}
