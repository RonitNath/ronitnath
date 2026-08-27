//! The member tier's group result sets: the groups a person belongs to, who
//! else is in them, and the invitations they have minted.
//!
//! Three statements, all keyed on the asking person's own membership. There is
//! no `group` parameter anywhere here and that is deliberate: a query that took
//! one would need an authorisation, and "the groups I am in" needs none — the
//! `membership` row *is* the authorisation, so a stranger's group cannot be
//! addressed at all rather than being addressed and refused.
//!
//! Contact scoping is not re-implemented. Whose details are visible inside a
//! group is [`visible_contacts`](rn_kernel::relation::visible_contacts) —
//! `person:P #contact @group:G` and nothing else — and this module asks it once
//! per group the reader belongs to rather than writing the rule a second time.

use rn_kernel::bind;
use rn_kernel::ids::{Group, Id, IdKey, Identity, Link, Person};
use rn_kernel::principal::SubjectSet;
use rn_kernel::relation::visible_contacts;
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::Row;
use crate::api::party::public_of;
use rn_kernel::domain::{PartyKind, Vocabulary as _};

/// The groups one party belongs to, with the role the membership row records
/// and the party that owns the group.
///
/// Rides `membership_party_idx (party_id, role)`, then the `party_resource`
/// and `resource` primary keys.
pub const GROUPS_SQL: &str = "SELECT p.id AS id, p.display_name AS display, \
     m.role AS role, m.at AS joined_at, \
     owner.id AS owner_id, owner.kind AS owner_kind, owner.display_name AS owner_display \
     FROM membership m \
     JOIN party p ON p.id = m.group_id AND p.kind = 'group' \
     JOIN party_resource pr ON pr.party_id = p.id \
     JOIN resource r ON r.id = pr.resource_id \
     JOIN party owner ON owner.id = r.owner_party_id \
     WHERE m.party_id = $1 \
     ORDER BY p.display_name, p.id";

/// Everyone in every group the asking party belongs to.
///
/// The reader's own membership is the driving row — `membership_party_idx`
/// again — and the group's roster is then the `membership` primary key under
/// its `group_id` prefix. A group the reader is not in is never reached.
pub const MEMBERS_SQL: &str = "SELECT m.group_id AS group_id, m.party_id AS party_id, \
     m.role AS role, m.at AS joined_at, p.kind AS party_kind, p.display_name AS display \
     FROM membership mine \
     JOIN party g ON g.id = mine.group_id AND g.kind = 'group' \
     JOIN membership m ON m.group_id = mine.group_id \
     JOIN party p ON p.id = m.party_id \
     WHERE mine.party_id = $1 \
     ORDER BY m.group_id, m.at, m.party_id";

/// The invitations one identity has minted.
///
/// An invitation is a `link` row plus the grant that says what holding its
/// token is worth, and `relation.granted_by` is the only record of who minted
/// it — so the grant is the driving row and the link hangs off it.
pub const INVITATIONS_SQL: &str = "SELECT l.id AS id, l.expires_at AS expires_at, \
     l.claimed_at AS claimed_at, l.created_at AS created_at, \
     rel.object_id AS container_id, rel.relation AS role, \
     g.kind AS container_kind, g.display_name AS container_display, \
     claimer.display_name AS claimed_display \
     FROM relation rel \
     JOIN link l ON l.id = rel.subject_id \
     JOIN party g ON g.id = rel.object_id \
     LEFT JOIN identity ci ON ci.id = l.claimed_by_identity_id \
     LEFT JOIN party claimer ON claimer.id = ci.person_id \
     WHERE rel.subject_kind = 'link' AND rel.granted_by = $1 \
     ORDER BY l.created_at DESC, l.id DESC";

struct GroupSummary {
    id: Id<Group>,
    display: String,
    role: String,
    joined_at: Timestamp,
    owner_id: i64,
    owner_kind: String,
    owner_display: String,
}

impl FromRow for GroupSummary {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            display: row.text("display")?,
            role: row.text("role")?,
            joined_at: row.int("joined_at")?,
            owner_id: row.int("owner_id")?,
            owner_kind: row.text("owner_kind")?,
            owner_display: row.text("owner_display")?,
        })
    }
}

/// The groups the acting person belongs to.
pub(super) async fn groups(
    reads: &impl Reads,
    subjects: &SubjectSet,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    let Some(person) = subjects.person else {
        return Ok(Vec::new());
    };
    Ok(reads
        .query::<GroupSummary>(GROUPS_SQL, bind![person])
        .await?
        .into_iter()
        .map(|row| {
            let public_id = row.id.public(key);
            Row {
                key: public_id.as_str().to_owned(),
                value: json!({
                    "public_id": public_id,
                    "display": row.display,
                    "role": row.role,
                    "joined_at": row.joined_at,
                    "owner": owner_ref(&row.owner_kind, row.owner_id, &row.owner_display, key),
                }),
            }
        })
        .collect())
}

fn owner_ref(kind: &str, id: i64, display: &str, key: &IdKey) -> serde_json::Value {
    match PartyKind::parse(kind) {
        Some(kind) => json!({
            "public_id": public_of(kind, id, key),
            "kind": kind.as_str(),
            "display": display,
        }),
        None => json!({ "public_id": serde_json::Value::Null, "kind": kind, "display": display }),
    }
}

struct MemberSummary {
    group_id: Id<Group>,
    party_id: Id<Person>,
    role: String,
    joined_at: Timestamp,
    party_kind: String,
    display: String,
}

impl FromRow for MemberSummary {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            group_id: row.id("group_id")?,
            party_id: row.id("party_id")?,
            role: row.text("role")?,
            joined_at: row.int("joined_at")?,
            party_kind: row.text("party_kind")?,
            display: row.text("display")?,
        })
    }
}

/// Everyone in the groups the acting person belongs to.
///
/// The `contact` flag is the answer to "may I see this person's details here",
/// asked of the kernel's own rule once per group.
pub(super) async fn group_members(
    reads: &impl Reads,
    subjects: &SubjectSet,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    let Some(person) = subjects.person else {
        return Ok(Vec::new());
    };
    let rows = reads
        .query::<MemberSummary>(MEMBERS_SQL, bind![person])
        .await?;

    let mut contacts: Vec<(Id<Group>, Id<Person>)> = Vec::new();
    let mut asked: Vec<Id<Group>> = Vec::new();
    for row in &rows {
        if asked.contains(&row.group_id) {
            continue;
        }
        asked.push(row.group_id);
        for visible in visible_contacts(reads, subjects, row.group_id).await? {
            contacts.push((row.group_id, visible));
        }
    }

    Ok(rows
        .into_iter()
        .map(|row| {
            let group = row.group_id.public(key);
            let party = PartyKind::parse(&row.party_kind)
                .map(|kind| public_of(kind, row.party_id.get(), key));
            let party_id = party.as_ref().map_or_else(String::new, |id| id.to_string());
            Row {
                key: format!("{}:{party_id}", group.as_str()),
                value: json!({
                    "group": group,
                    "public_id": party,
                    "kind": row.party_kind,
                    "display": row.display,
                    "role": row.role,
                    "joined_at": row.joined_at,
                    "you": subjects.person == Some(row.party_id),
                    "contact": contacts.contains(&(row.group_id, row.party_id)),
                }),
            }
        })
        .collect())
}

struct InvitationSummary {
    id: Id<Link>,
    expires_at: Timestamp,
    claimed_at: Option<Timestamp>,
    created_at: Timestamp,
    container_id: i64,
    role: String,
    container_kind: String,
    container_display: String,
    claimed_display: Option<String>,
}

impl FromRow for InvitationSummary {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            expires_at: row.int("expires_at")?,
            claimed_at: row.int_opt("claimed_at")?,
            created_at: row.int("created_at")?,
            container_id: row.int("container_id")?,
            role: row.text("role")?,
            container_kind: row.text("container_kind")?,
            container_display: row.text("container_display")?,
            claimed_display: row.text_opt("claimed_display")?,
        })
    }
}

/// The invitations the acting identity minted.
///
/// The token is not here: it was handed back once, at mint, and the row keeps
/// only its SHA-256. The link's *public id* is, because naming the row and
/// naming the secret are two different things — it is what `RevokeLink` takes,
/// and it is what lets somebody withdraw an invitation they sent by mistake.
///
/// The key is still the row's position in the newest-first list rather than
/// that id, because the list is what a client renders in order.
pub(super) async fn invitations(
    reads: &impl Reads,
    identity: Id<Identity>,
    key: &IdKey,
) -> Outcome<Vec<Row>> {
    Ok(reads
        .query::<InvitationSummary>(INVITATIONS_SQL, bind![identity])
        .await?
        .into_iter()
        .enumerate()
        .map(|(at, row)| Row {
            // Zero-padded so a key-ordered client map reads in list order
            // rather than lexicographically wrong at every power of ten.
            key: format!("{at:020}"),
            value: json!({
                "id": row.id.public(key),
                "created_at": row.created_at,
                "expires_at": row.expires_at,
                "claimed_at": row.claimed_at,
                "claimed_by": row.claimed_display,
                "role": row.role,
                "container": owner_ref(
                    &row.container_kind,
                    row.container_id,
                    &row.container_display,
                    key,
                ),
            }),
        })
        .collect())
}
