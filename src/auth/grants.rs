//! Per-resource sharing: grants, share roles, and access resolution.
//!
//! Two layers answer two different questions, and they must not blur:
//!
//! - [`Capability`](super::capability::Capability) (membership layer): what a
//!   session may do *in its account* — coarse, structural.
//! - [`ShareRole`] (this module): what a session may do *to one resource* —
//!   fine-grained, per object, stored in `resource_grants`.
//!
//! Sharing a resource never grants account capabilities, and holding an account
//! capability never silently confers access to another account's resource; any
//! such rule would have to be written here, visibly.
//!
//! A share role is a bundle over per-resource actions, expanded in code the
//! same way membership role bundles are: the row stores one word, a `match`
//! the compiler checks says what it means.

use hiqlite::{Client, params};
use tracing::{info, instrument};

use super::ids::InternalId;
use super::session::now_ms;
use super::store::{SessionContext, StoreError};

/// Every kind of resource a grant can name.
///
/// Closed enum for the same reason [`Capability`](super::Capability) is one: a
/// grant against a kind that does not exist is unrepresentable, and a new kind
/// forces every match to say what it means. The list grows exactly when a new
/// resource table lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    /// Writing/blog documents — the presence project's first shareable
    /// resource. The grants infrastructure lands ahead of the documents table
    /// so the table's design can assume it.
    Document,
}

impl ResourceKind {
    pub const ALL: &[Self] = &[Self::Document];

    /// The stored form. Stable: rows hold these strings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Document => "document",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.as_str() == s)
    }
}

impl std::fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What one grant lets its subject do. Ordered: each role contains the ones
/// below it, and `Ord` is what lets resolution take the strongest match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShareRole {
    Viewer,
    Commenter,
    Editor,
}

impl ShareRole {
    pub const ALL: &[Self] = &[Self::Viewer, Self::Commenter, Self::Editor];

    /// The stored form. Stable: rows hold these strings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Commenter => "commenter",
            Self::Editor => "editor",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|r| r.as_str() == s)
    }

    /// The action bundle. Roles are strictly nested — an editor can do
    /// everything a commenter can — and the nesting lives here, nowhere else.
    #[must_use]
    pub const fn allows(self, action: ResourceAction) -> bool {
        match action {
            ResourceAction::Read => true,
            ResourceAction::Comment => matches!(self, Self::Commenter | Self::Editor),
            ResourceAction::Write => matches!(self, Self::Editor),
        }
    }
}

impl std::fmt::Display for ShareRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single thing a request wants to do to a resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceAction {
    Read,
    Comment,
    Write,
}

/// Who a grant is for.
///
/// The schema also admits `'link'` for capability links; the variant appears
/// here when the links table exists, because a link grant is only resolvable
/// against a presented token, which nothing can present yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantSubject {
    /// Shared with a person: follows the identity across all of its accounts.
    Identity(InternalId),
    /// Shared with an account: everyone whose session is on it, at once.
    Account(InternalId),
}

impl GrantSubject {
    const fn kind_str(self) -> &'static str {
        match self {
            Self::Identity(_) => "identity",
            Self::Account(_) => "account",
        }
    }

    const fn id(self) -> InternalId {
        match self {
            Self::Identity(id) | Self::Account(id) => id,
        }
    }
}

/// How a session reaches a resource, strongest source wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// The session's account owns the resource: every action, no grant row.
    Owner,
    /// Reached through a grant; the role bounds the actions.
    Shared(ShareRole),
}

impl Access {
    #[must_use]
    pub const fn allows(self, action: ResourceAction) -> bool {
        match self {
            Self::Owner => true,
            Self::Shared(role) => role.allows(action),
        }
    }
}

/// Create or update a grant. One row per (resource, subject): re-granting with
/// a different role replaces it, so "change Alex to editor" is a grant, not a
/// revoke-then-grant with a gap in between.
#[instrument(name = "grants.grant", skip(db))]
pub async fn grant(
    db: &Client,
    kind: ResourceKind,
    resource_public_id: &str,
    subject: GrantSubject,
    role: ShareRole,
    granted_by: InternalId,
) -> Result<(), StoreError> {
    db.execute(
        "INSERT INTO resource_grants
           (resource_kind, resource_public_id, subject_kind, subject_id, role,
            granted_by_identity_id, granted_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (resource_kind, resource_public_id, subject_kind, subject_id)
         DO UPDATE SET role = excluded.role,
                       granted_by_identity_id = excluded.granted_by_identity_id,
                       granted_at = excluded.granted_at",
        params!(
            kind.as_str(),
            resource_public_id.to_string(),
            subject.kind_str(),
            subject.id().get(),
            role.as_str(),
            granted_by.get(),
            now_ms()
        ),
    )
    .await?;
    info!(
        kind = kind.as_str(),
        subject = subject.kind_str(),
        role = role.as_str(),
        "resource grant written"
    );
    Ok(())
}

/// Remove a grant. Idempotent: revoking an absent grant reports `false`, the
/// same shape as session revocation.
#[instrument(name = "grants.revoke", skip(db))]
pub async fn revoke(
    db: &Client,
    kind: ResourceKind,
    resource_public_id: &str,
    subject: GrantSubject,
) -> Result<bool, StoreError> {
    let affected = db
        .execute(
            "DELETE FROM resource_grants
             WHERE resource_kind = $1 AND resource_public_id = $2
               AND subject_kind = $3 AND subject_id = $4",
            params!(
                kind.as_str(),
                resource_public_id.to_string(),
                subject.kind_str(),
                subject.id().get()
            ),
        )
        .await?;
    info!(revoked = affected, "resource grant revoke finished");
    Ok(affected > 0)
}

/// Resolve what `session` may do to one resource.
///
/// `owner_account_id` comes from the loaded resource row — resolution always
/// starts from a resource the caller already fetched, which is also why the
/// grant table can name resources by public id without a join back.
///
/// Effective access is the strongest of:
/// 1. ownership — the session's account is the resource's account;
/// 2. an identity grant for the session's identity;
/// 3. an account grant for the session's account.
///
/// `None` means the resource must behave as if it does not exist for this
/// session — not merely decline the action.
#[instrument(name = "grants.resolve_access", skip(db, session))]
pub async fn resolve_access(
    db: &Client,
    kind: ResourceKind,
    resource_public_id: &str,
    owner_account_id: InternalId,
    session: &SessionContext,
) -> Result<Option<Access>, StoreError> {
    if session.account_id == owner_account_id {
        return Ok(Some(Access::Owner));
    }

    #[derive(serde::Deserialize)]
    struct RoleRow {
        role: String,
    }
    let rows: Vec<RoleRow> = db
        .query_as(
            "SELECT role FROM resource_grants
             WHERE resource_kind = $1 AND resource_public_id = $2
               AND ((subject_kind = 'identity' AND subject_id = $3)
                 OR (subject_kind = 'account'  AND subject_id = $4))",
            params!(
                kind.as_str(),
                resource_public_id.to_string(),
                session.identity_id.get(),
                session.account_id.get()
            ),
        )
        .await?;

    // Unknown role strings cannot pass the schema CHECK; one here means the
    // row and the code disagree about the schema.
    let strongest = rows
        .iter()
        .map(|r| {
            ShareRole::parse(&r.role)
                .ok_or(StoreError::Corrupt("grant role is not a known share role"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max();

    Ok(strongest.map(Access::Shared))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_round_trip() {
        for role in ShareRole::ALL {
            assert_eq!(ShareRole::parse(role.as_str()), Some(*role));
        }
        for kind in ResourceKind::ALL {
            assert_eq!(ResourceKind::parse(kind.as_str()), Some(*kind));
        }
        assert_eq!(ShareRole::parse("owner"), None, "ownership is not a grant");
    }

    #[test]
    fn roles_nest_strictly() {
        use ResourceAction::{Comment, Read, Write};
        assert!(ShareRole::Viewer.allows(Read));
        assert!(!ShareRole::Viewer.allows(Comment));
        assert!(!ShareRole::Viewer.allows(Write));
        assert!(ShareRole::Commenter.allows(Read));
        assert!(ShareRole::Commenter.allows(Comment));
        assert!(!ShareRole::Commenter.allows(Write));
        assert!(ShareRole::Editor.allows(Read));
        assert!(ShareRole::Editor.allows(Comment));
        assert!(ShareRole::Editor.allows(Write));
    }

    #[test]
    fn role_ordering_matches_strength() {
        assert!(ShareRole::Viewer < ShareRole::Commenter);
        assert!(ShareRole::Commenter < ShareRole::Editor);
    }

    #[test]
    fn owner_access_allows_everything() {
        for action in [
            ResourceAction::Read,
            ResourceAction::Comment,
            ResourceAction::Write,
        ] {
            assert!(Access::Owner.allows(action));
        }
        assert!(Access::Shared(ShareRole::Editor).allows(ResourceAction::Write));
        assert!(!Access::Shared(ShareRole::Viewer).allows(ResourceAction::Write));
    }
}
