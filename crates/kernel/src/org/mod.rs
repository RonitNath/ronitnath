//! Organizations, and the membership machinery both containers share.
//!
//! ## The shape an organization has
//!
//! An organization is three rows, and each one carries a different fact:
//!
//! ```text
//!   party(kind='organization', display_name, status)     what is granted and belongs
//!   resource(kind='organization', owner_party_id=P)      what is owned
//!   party_resource(resource_id, party_id)                the 1:1 join between them
//! ```
//!
//! plus one `membership(group_id = the org's own party id, party_id = P,
//! role = 'owner')` for the founder. **An organization is also its own root
//! group**: "the members of org O" is `membership` rows whose `group_id` is
//! O's party id, which is why one membership table and one role vocabulary
//! cover organizations and groups alike, and why `principal::expand` reads
//! both out of the same query.
//!
//! A group has exactly the same shape with `kind='group'`, and its
//! `resource.owner_party_id` is the person who made it or the organization it
//! was made under. Groups never own: migration 1 has a trigger that refuses a
//! group as `owner_party_id`, so "groups are granted, never owners" is
//! unrepresentable rather than remembered.
//!
//! ## Ownership versus administration
//!
//! Two things could both be called "who owns this organization", so they are
//! named apart:
//!
//! * `resource.owner_party_id` is **ownership** — the tenant boundary, the
//!   party a transfer moves it from. It changes only through `Transfer`.
//! * `membership.role = 'owner'` is **administration** — who may set roles,
//!   invite, and refuse to be the last one out. `SetRole` moves the others;
//!   nobody reaches `owner` through it.
//!
//! `Transfer` keeps them agreeing for the party-backed kinds: it moves the
//! owner membership along with the column, and demotes the party it took it
//! from to `admin` rather than removing it.

#[cfg(test)]
mod tests;

use crate::domain::{PartyRow, Vocabulary};
use crate::error::Outcome;
use crate::ids::{Id, Person};
use crate::relation::Relation;
use crate::store::{Count, Cursor, FromRow, Reads, RowError};
use crate::{Timestamp, bind};

/// A party's role in a container. `member < admin < owner`, nested in code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberRole {
    /// Belongs.
    Member,
    /// Belongs and administers membership.
    Admin,
    /// Belongs, administers, and is the party a transfer moves from.
    Owner,
}

impl Vocabulary for MemberRole {
    const ALL: &'static [Self] = &[Self::Member, Self::Admin, Self::Owner];
    const COLUMN: (&'static str, &'static str) = ("membership", "role");

    fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Admin => "admin",
            Self::Owner => "owner",
        }
    }
}

impl MemberRole {
    /// The relation this role is, so nesting is decided in one place for
    /// membership rows and relation rows alike.
    pub const fn relation(self) -> Relation {
        match self {
            Self::Member => Relation::Member,
            Self::Admin => Relation::Admin,
            Self::Owner => Relation::Owner,
        }
    }

    /// Whether holding this role is enough to act as `wanted`.
    pub const fn covers(self, wanted: Self) -> bool {
        self.relation().covers(wanted.relation())
    }
}

impl From<rn_api::whoami::MemberRole> for MemberRole {
    fn from(role: rn_api::whoami::MemberRole) -> Self {
        match role {
            rn_api::whoami::MemberRole::Member => Self::Member,
            rn_api::whoami::MemberRole::Admin => Self::Admin,
            rn_api::whoami::MemberRole::Owner => Self::Owner,
        }
    }
}

impl From<MemberRole> for rn_api::whoami::MemberRole {
    fn from(role: MemberRole) -> Self {
        match role {
            MemberRole::Member => Self::Member,
            MemberRole::Admin => Self::Admin,
            MemberRole::Owner => Self::Owner,
        }
    }
}

/// One membership row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipRow {
    /// The container's party id.
    pub group_id: Id<Person>,
    /// The member's party id.
    pub party_id: Id<Person>,
    /// What they hold.
    pub role: MemberRole,
    /// When they joined, unix seconds.
    pub at: Timestamp,
}

impl FromRow for MembershipRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let role = row.text("role")?;
        Ok(Self {
            group_id: row.id("group_id")?,
            party_id: row.id("party_id")?,
            role: MemberRole::parse(&role).ok_or(RowError::Column {
                column: "role".to_owned(),
                wanted: "MemberRole",
            })?,
            at: row.int("at")?,
        })
    }
}

/// One party's role in one container: the primary key, exactly.
pub const ROLE_SQL: &str =
    "SELECT group_id, party_id, role, at FROM membership WHERE group_id = $1 AND party_id = $2";

/// Everyone in a container, with the display name a UI needs.
pub const MEMBERS_SQL: &str = "SELECT m.group_id, m.party_id, m.role, m.at, \
                               p.kind, p.display_name, p.status, p.created_at, p.id \
                               FROM membership m JOIN party p ON p.id = m.party_id \
                               WHERE m.group_id = $1 ORDER BY m.at, m.party_id";

/// How many owners a container has, which is what stops the last one leaving.
pub const OWNER_COUNT_SQL: &str =
    "SELECT count(*) AS n FROM membership WHERE group_id = $1 AND role = 'owner'";

/// Add a membership.
pub const INSERT_SQL: &str =
    "INSERT INTO membership (group_id, party_id, role, at) VALUES ($1, $2, $3, $4)";

/// Add a membership, or leave the one already there alone — what claiming a
/// link a second time means.
pub const UPSERT_SQL: &str = "INSERT INTO membership (group_id, party_id, role, at) \
                              VALUES ($1, $2, $3, $4) \
                              ON CONFLICT (group_id, party_id) DO NOTHING";

/// Change a role. Guarded on the role that was read, so a concurrent change
/// writes nothing rather than clobbering.
pub const SET_ROLE_SQL: &str =
    "UPDATE membership SET role = $1 WHERE group_id = $2 AND party_id = $3 AND role = $4";

/// Force a role regardless of the current one — `Transfer`'s half of keeping
/// ownership and administration in step.
pub const FORCE_ROLE_SQL: &str = "UPDATE membership SET role = $1 \
                                  WHERE group_id = $2 AND party_id = $3";

/// Add or overwrite a membership at a role.
pub const UPSERT_ROLE_SQL: &str = "INSERT INTO membership (group_id, party_id, role, at) \
                                   VALUES ($1, $2, $3, $4) \
                                   ON CONFLICT (group_id, party_id) DO UPDATE SET role = $3";

/// Remove a membership.
pub const DELETE_SQL: &str = "DELETE FROM membership WHERE group_id = $1 AND party_id = $2";

/// A member of a container, as a listing renders them.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    /// The membership row.
    pub membership: MembershipRow,
    /// The party it names.
    pub party: PartyRow,
}

impl FromRow for Member {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        // `MembershipRow` first: both structs read `id`-shaped columns, and
        // the cursor consumes a column as it converts it.
        Ok(Self {
            membership: MembershipRow::from_row(row)?,
            party: PartyRow::from_row(row)?,
        })
    }
}

/// What role a party holds in a container, if any.
pub async fn role_of(
    store: &impl Reads,
    container: Id<Person>,
    party: Id<Person>,
) -> Outcome<Option<MemberRole>> {
    Ok(store
        .query_opt::<MembershipRow>(ROLE_SQL, bind![container, party])
        .await?
        .map(|row| row.role))
}

/// Whether a party holds at least `role` in a container.
pub async fn holds(
    store: &impl Reads,
    container: Id<Person>,
    party: Id<Person>,
    role: MemberRole,
) -> Outcome<bool> {
    Ok(role_of(store, container, party)
        .await?
        .is_some_and(|held| held.covers(role)))
}

/// Everyone in a container.
pub async fn members(store: &impl Reads, container: Id<Person>) -> Outcome<Vec<Member>> {
    Ok(store.query::<Member>(MEMBERS_SQL, bind![container]).await?)
}

/// How many owners a container has.
pub async fn owner_count(store: &impl Reads, container: Id<Person>) -> Outcome<i64> {
    Ok(store
        .query::<Count>(OWNER_COUNT_SQL, bind![container])
        .await?
        .first()
        .map_or(0, |c| c.0))
}
