//! The organization, its members, its groups and its contact scoping.
//!
//! Every statement here is anchored on one of two seeks: `membership` by its
//! primary key `(group_id, party_id)`, or `resource` by
//! `resource_owner_idx (owner_party_id, kind, status)`. The organization's
//! party id is the anchor in both, which is what makes "everything of this
//! tenant" a range rather than a filter over the deployment — see
//! `tests/org.rs`, where each of them is asserted under `EXPLAIN QUERY PLAN`.

use rn_kernel::bind;
use rn_kernel::domain::{PartyKind, PartyStatus, Vocabulary as _};
use rn_kernel::error::{Outcome, decline};
use rn_kernel::ids::{Group, Id, IdKey, Person, Resource};
use rn_kernel::org::{self, Member, MemberRole};
use rn_kernel::relation::Subject;
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError};
use serde_json::json;

use super::org::Scope;
use super::{Params, Row};
use crate::api::party::public_of;

/// The organization's own two halves, and the party that owns it.
pub const OVERVIEW: &str = "SELECT p.id, p.kind, p.display_name, p.status, p.created_at, \
     r.id AS resource_id, r.owner_party_id, \
     owner.kind AS owner_kind, owner.display_name AS owner_display \
     FROM party p \
     JOIN party_resource pr ON pr.party_id = p.id \
     JOIN resource r ON r.id = pr.resource_id \
     JOIN party owner ON owner.id = r.owner_party_id \
     WHERE p.id = $1 AND p.kind = 'organization'";

/// How many parties belong to a container. The primary key answers it.
pub const MEMBER_COUNT: &str = "SELECT count(*) AS n FROM membership WHERE group_id = $1";

/// How many live resources of one kind this party owns.
pub const OWNED_COUNT: &str = "SELECT count(*) AS n FROM resource \
     WHERE owner_party_id = $1 AND kind = $2 AND status <> 'deleted'";

/// The groups an organization owns, with the size of each.
pub const GROUPS: &str = "SELECT p.id, p.kind, p.display_name, p.status, p.created_at, \
     r.id AS resource_id, \
     (SELECT count(*) FROM membership m WHERE m.group_id = p.id) AS members \
     FROM resource r \
     JOIN party_resource pr ON pr.resource_id = r.id \
     JOIN party p ON p.id = pr.party_id \
     WHERE r.owner_party_id = $1 AND r.kind = 'group' AND r.status <> 'deleted' \
     ORDER BY p.display_name, p.id";

/// Whose contact details are visible inside one group.
pub const CONTACTS: &str = "SELECT p.id, p.kind, p.display_name, p.status, p.created_at \
     FROM relation r JOIN party p ON p.id = r.object_id \
     WHERE r.subject_key = $1 AND r.object_kind = 'person' AND r.relation = 'contact' \
     ORDER BY p.display_name, p.id";

/// The organization as its overview page renders it: who owns it, when it was
/// founded, and the three counts the section titles carry.
pub async fn overview(reads: &impl Reads, scope: Scope) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    let Some(row) = reads
        .query_opt::<Overview>(OVERVIEW, bind![scope.party])
        .await?
    else {
        return decline();
    };
    let members = counted(reads, MEMBER_COUNT, bind![scope.party]).await?;
    let groups = counted(reads, OWNED_COUNT, bind![scope.party, "group"]).await?;
    let documents = counted(reads, OWNED_COUNT, bind![scope.party, "document"]).await?;
    let public_id = scope.org.public(key);
    Ok(vec![Row {
        key: public_id.as_str().to_owned(),
        value: json!({
            "public_id": public_id,
            // What `Transfer` addresses. The party id above is what
            // memberships and invitations address; they are different rows and
            // a page that offered one where the other is wanted would decline.
            "resource": row.resource.public(key),
            "display": row.display,
            "status": row.status.as_str(),
            "created_at": row.created_at,
            "owner": {
                "public_id": public_of(row.owner_kind, row.owner.get(), key),
                "display": row.owner_display,
            },
            "members": members,
            "groups": groups,
            "documents": documents,
            "my_role": scope.role.as_str(),
            "owned_by_me": row.owner == scope.person,
        }),
    }])
}

/// The memberships of a container: the organization itself by default, or one
/// of its groups when the subscription names one.
pub async fn members(reads: &impl Reads, scope: Scope, params: &Params) -> Outcome<Vec<Row>> {
    let container = container_of(reads, scope, params).await?;
    let key = reads.ids();
    let owners = org::owner_count(reads, container).await?;
    Ok(org::members(reads, container)
        .await?
        .into_iter()
        .map(|member| member_row(&member, owners, scope, key))
        .collect())
}

/// The groups this organization owns.
pub async fn groups(reads: &impl Reads, scope: Scope) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    Ok(reads
        .query::<GroupSummary>(GROUPS, bind![scope.party])
        .await?
        .into_iter()
        .map(|row| {
            let public_id = row.group.public(key);
            Row {
                key: public_id.as_str().to_owned(),
                value: json!({
                    "public_id": public_id,
                    "resource": row.resource.public(key),
                    "display": row.display,
                    "status": row.status.as_str(),
                    "created_at": row.created_at,
                    "members": row.members,
                }),
            }
        })
        .collect())
}

/// The persons who have shared their contact details into one of this
/// organization's groups.
pub async fn contacts(reads: &impl Reads, scope: Scope, params: &Params) -> Outcome<Vec<Row>> {
    let Some(named) = params.group.as_ref() else {
        return decline();
    };
    let group = super::org::group_of(reads, scope, named).await?;
    let key = reads.ids();
    Ok(reads
        .query::<rn_kernel::domain::PartyRow>(CONTACTS, bind![Subject::group(group).key()])
        .await?
        .into_iter()
        .map(|party| {
            let public_id = public_of(party.kind, party.id.get(), key);
            Row {
                key: public_id.as_str().to_owned(),
                value: json!({
                    "public_id": public_id,
                    "display": party.display_name,
                    "status": party.status.as_str(),
                }),
            }
        })
        .collect())
}

/// The group party ids this organization owns — what the audit and invitation
/// queries scope themselves by.
pub async fn group_ids(reads: &impl Reads, scope: Scope) -> Outcome<Vec<Id<Group>>> {
    Ok(reads
        .query::<GroupSummary>(GROUPS, bind![scope.party])
        .await?
        .into_iter()
        .map(|row| row.group)
        .collect())
}

/// The container a membership listing is about, proven to belong to this
/// organization when it is not the organization itself.
async fn container_of(reads: &impl Reads, scope: Scope, params: &Params) -> Outcome<Id<Person>> {
    match params.group.as_ref() {
        Some(named) => super::org::group_of(reads, scope, named)
            .await
            .map(|group| Id::new(group.get())),
        None => Ok(scope.party),
    }
}

/// One membership, as the table renders it.
///
/// `demotable` is the fact the row needs that the row does not contain: the
/// last owner of a container cannot be demoted, and a select that discovered
/// that only by being refused would be a control that lies until you use it.
fn member_row(member: &Member, owners: i64, scope: Scope, key: &IdKey) -> Row {
    let party = &member.party;
    let public_id = public_of(party.kind, party.id.get(), key);
    let is_owner = member.membership.role == MemberRole::Owner;
    Row {
        key: public_id.as_str().to_owned(),
        value: json!({
            "public_id": public_id,
            "display": party.display_name,
            "kind": party.kind.as_str(),
            "status": party.status.as_str(),
            "role": member.membership.role.as_str(),
            "since": member.membership.at,
            "me": party.id == scope.person,
            // An admin moves members; only an owner moves an owner.
            "settable": if is_owner {
                scope.role == MemberRole::Owner && owners > 1
            } else {
                scope.role.covers(MemberRole::Admin)
            },
            "last_owner": is_owner && owners <= 1,
        }),
    }
}

async fn counted(
    reads: &impl Reads,
    sql: &'static str,
    params: Vec<rn_kernel::store::Value>,
) -> Outcome<i64> {
    Ok(reads
        .query::<Count>(sql, params)
        .await?
        .first()
        .map_or(0, |count| count.0))
}

struct Overview {
    display: String,
    status: PartyStatus,
    created_at: i64,
    resource: Id<Resource>,
    owner: Id<Person>,
    owner_kind: PartyKind,
    owner_display: String,
}

impl FromRow for Overview {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let party = rn_kernel::domain::PartyRow::from_row(row)?;
        let owner_kind = row.text("owner_kind")?;
        Ok(Self {
            display: party.display_name,
            status: party.status,
            created_at: party.created_at,
            resource: row.id("resource_id")?,
            owner: row.id("owner_party_id")?,
            owner_kind: PartyKind::parse(&owner_kind).ok_or(RowError::Column {
                column: "owner_kind".to_owned(),
                wanted: "PartyKind",
            })?,
            owner_display: row.text("owner_display")?,
        })
    }
}

struct GroupSummary {
    group: Id<Group>,
    display: String,
    status: PartyStatus,
    created_at: i64,
    resource: Id<Resource>,
    members: i64,
}

impl FromRow for GroupSummary {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let party = rn_kernel::domain::PartyRow::from_row(row)?;
        Ok(Self {
            group: Id::new(party.id.get()),
            display: party.display_name,
            status: party.status,
            created_at: party.created_at,
            resource: row.id("resource_id")?,
            members: row.int("members")?,
        })
    }
}
