//! Invitations: a bearer link that carries one role into one container.
//!
//! An invitation is a `link` row — a 256-bit secret stored only as its
//! SHA-256, with an expiry — plus a relation row saying what holding it is
//! worth: `group:X #member @link:T`. The role is not a column on `link`, and
//! that is the point. A link is a *subject* like any other, so "what does this
//! token get you" is answered by the same relation store as every other
//! question, and an invitation that is revoked is a relation row that is
//! deleted.
//!
//! Claiming is single-use and the database is what makes it so: the update
//! that marks the link claimed carries `claimed_by_identity_id IS NULL AND
//! expires_at > now` in its `WHERE`, and every other statement in the
//! command's batch carries the same guard — so a token claimed twice writes
//! nothing the second time rather than half of something.

use crate::Timestamp;
use crate::domain::LinkRow;
use crate::ids::{Group, Id, Identity, Link};
use crate::org::MemberRole;
use crate::relation::Relation;
use crate::store::{Cursor, FromRow, RowError};

/// The longest an invitation may last: a month. A caller may ask for less and
/// the command takes the earlier of the two — an invitation that never expires
/// is a password with a friendly name.
pub const MAX_TTL: i64 = 60 * 60 * 24 * 30;

/// What a token turns out to be worth.
#[derive(Debug, Clone, PartialEq)]
pub struct Invitation {
    /// The link row.
    pub link: LinkRow,
    /// The container it joins — a group's or an organization's party id.
    pub container: Id<Group>,
    /// The role it lands the claimant on.
    pub role: MemberRole,
}

impl Invitation {
    /// The link's id.
    pub const fn id(&self) -> Id<Link> {
        self.link.id
    }

    /// Whether this invitation may still be claimed at `now`.
    pub const fn is_claimable(&self, now: Timestamp) -> bool {
        self.link.is_claimable(now)
    }
}

impl FromRow for Invitation {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let container = row.id("container_id")?;
        let role = row.text("role")?;
        Ok(Self {
            link: LinkRow {
                id: row.id("id")?,
                expires_at: row.int("expires_at")?,
                claimed_by_identity_id: row.id_opt("claimed_by_identity_id")?,
                verifies_factor_id: None,
            },
            container,
            role: match Relation::parse(&role) {
                Some(Relation::Member) => MemberRole::Member,
                Some(Relation::Admin) => MemberRole::Admin,
                // An invitation to own is not a thing `Invite` can mint:
                // ownership moves through `Transfer` and nowhere else.
                _ => {
                    return Err(RowError::Column {
                        column: "role".to_owned(),
                        wanted: "MemberRole",
                    });
                }
            },
        })
    }
}

/// What a token is worth, in one query.
///
/// The link is reached by its digest through the UNIQUE index on
/// `link.token_hash`; the grant is reached through `relation_subject_idx`.
/// Both are seeks, which matters because this runs on a public page anybody
/// can post a guess to.
pub const LOOKUP_SQL: &str = "SELECT l.id, l.expires_at, l.claimed_by_identity_id, \
     rel.object_id AS container_id, rel.relation AS role \
     FROM link l \
     JOIN relation rel ON rel.subject_kind = 'link' AND rel.subject_id = l.id \
     WHERE l.token_hash = $1 AND rel.object_kind IN ('group', 'organization')";

/// What an invitation is, in words a page can print.
///
/// The same relation row [`Invitation`] resolves for the command, read for a
/// reader instead of for a batch: the container it joins, the role it lands
/// on, who minted it, and when it stops working. A claim page that cannot say
/// those four things is asking somebody to accept an unnamed thing from an
/// unnamed person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    /// The container's display name.
    pub container: String,
    /// `organization` or `group` — the `party.kind` of the container.
    pub container_kind: String,
    /// The role the claimant lands on.
    pub role: String,
    /// The display name of the person who minted it, if the identity that
    /// granted it has resolved to one.
    pub minted_by: Option<String>,
    /// When it stops working, unix seconds.
    pub expires_at: Timestamp,
}

impl FromRow for Description {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            container: row.text("container")?,
            container_kind: row.text("container_kind")?,
            role: row.text("role")?,
            minted_by: row.text_opt("minted_by")?,
            expires_at: row.int("expires_at")?,
        })
    }
}

/// The same two seeks [`LOOKUP_SQL`] makes — the link by rowid, the grant
/// through `relation_subject_idx` — plus the display names hanging off them.
/// The minter is a `LEFT JOIN`: a grant whose identity has not resolved to a
/// person still has a container and a role to name.
pub const DESCRIBE_SQL: &str = "SELECT c.display_name AS container, c.kind AS container_kind, \
     rel.relation AS role, minter.display_name AS minted_by, l.expires_at \
     FROM link l \
     JOIN relation rel ON rel.subject_kind = 'link' AND rel.subject_id = l.id \
     JOIN party c ON c.id = rel.object_id \
     LEFT JOIN identity i ON i.id = rel.granted_by \
     LEFT JOIN party minter ON minter.id = i.person_id \
     WHERE l.id = $1 AND rel.object_kind IN ('group', 'organization')";

/// Read what a token is worth, for a reader rather than for a command.
pub async fn describe(
    store: &impl crate::store::Reads,
    link: Id<Link>,
) -> crate::error::Outcome<Option<Description>> {
    Ok(store
        .query_opt::<Description>(DESCRIBE_SQL, crate::bind![link])
        .await?)
}

/// Mint the link half of an invitation.
pub const INSERT_SQL: &str =
    "INSERT INTO link (token_hash, expires_at, created_at) VALUES ($1, $2, $3) RETURNING id";

/// Mark a link claimed. The guard is the whole of "single use".
pub const CLAIM_SQL: &str = "UPDATE link SET claimed_by_identity_id = $1, claimed_at = $2 \
                             WHERE id = $3 AND claimed_by_identity_id IS NULL AND expires_at > $2";

/// Read what a token is worth.
pub async fn lookup(
    store: &impl crate::store::Reads,
    digest: [u8; 32],
) -> crate::error::Outcome<Option<Invitation>> {
    Ok(store
        .query_opt::<Invitation>(LOOKUP_SQL, crate::bind![digest.to_vec()])
        .await?)
}

/// The identity that claimed a link, for a caller rendering the outcome.
pub const fn claimed_by(invitation: &Invitation) -> Option<Id<Identity>> {
    invitation.link.claimed_by_identity_id
}
