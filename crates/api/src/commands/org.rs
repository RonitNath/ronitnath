//! Organizations, groups, membership and invitation links.

use serde::{Deserialize, Serialize};

use crate::ids::PublicId;
use crate::whoami::MemberRole;

/// Create an organization. The acting person becomes its owner; ownership
/// moves only through [`Transfer`](super::Transfer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateOrganization {
    /// How the organization is named in a UI.
    pub display_name: String,
}

/// Create a group: a set of parties, granted things but never owning them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateGroup {
    /// How the group is named in a UI.
    pub display_name: String,
    /// The organization it belongs to. Absent for a group the acting person
    /// keeps to themselves — a contact list, say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<PublicId>,
}

/// Mint an invitation link into a group.
///
/// The link is a `link` row whose relation is `group #<role> @link:T`, so a
/// bearer holds exactly the role the invitation named and nothing else. The
/// token is returned once, in the reply, and stored only as its SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    /// The group being joined.
    pub group: PublicId,
    /// The role the claimant lands on.
    pub role: MemberRole,
    /// When the link stops working, unix seconds. The server caps this.
    pub expires_at: i64,
}

/// Claim an invitation link, joining the acting identity to its group.
///
/// This is the one command a bearer principal may run, and it is reachable
/// only from the public claim page — never from `/api/*`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimLink {
    /// The bearer token from the link.
    pub token: String,
}

/// Change a party's role in a group or organization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetRole {
    /// The group or organization.
    pub group: PublicId,
    /// The member whose role changes.
    pub party: PublicId,
    /// The role they land on. Reaching `owner` this way is refused: that is
    /// what [`Transfer`](super::Transfer) is for.
    pub role: MemberRole,
}

/// Withdraw an invitation that has not been claimed.
///
/// The link is named by its public id, never by its token: an admin revoking
/// somebody else's invitation has no business holding the secret that would
/// let them claim it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeLink {
    /// The invitation.
    pub link: PublicId,
}

/// Leave a group or organization. The acting party removes its own membership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leave {
    /// The group or organization being left.
    pub group: PublicId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{id, round_trip};

    #[test]
    fn organization_commands_round_trip() {
        round_trip(&CreateOrganization {
            display_name: "Isoastra".into(),
        });
        round_trip(&CreateGroup {
            display_name: "Founders".into(),
            organization: Some(id("o_")),
        });
        round_trip(&Invite {
            group: id("g_"),
            role: MemberRole::Member,
            expires_at: 1_787_000_000,
        });
        round_trip(&ClaimLink {
            token: "opaque".into(),
        });
        round_trip(&SetRole {
            group: id("g_"),
            party: id("p_"),
            role: MemberRole::Admin,
        });
        round_trip(&RevokeLink { link: id("l_") });
        round_trip(&Leave { group: id("g_") });
    }

    #[test]
    fn a_personal_group_omits_the_organization() {
        let group = CreateGroup {
            display_name: "Contacts".into(),
            organization: None,
        };
        let json = serde_json::to_value(&group).expect("serialises");
        assert!(json.get("organization").is_none());
        round_trip(&group);
    }
}
