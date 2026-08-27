//! `GET /api/whoami` — the only chrome DTO.
//!
//! A bundle renders its shell from this and nothing else. It therefore carries
//! public ids, display names, the tiers the principal holds and the
//! organizations the chrome lists — and deliberately never an email address,
//! never an internal id, and no relation the chrome does not draw. Widening it
//! is how a "harmless" DTO becomes an enumeration endpoint, so a new field
//! needs a screen that renders it.

use serde::{Deserialize, Serialize};

use crate::ids::PublicId;

/// Which bundle the server serves and the principal holds. The tier *is* the
/// app: there is no role-filtered navigation inside one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// `/app` — any signed-in person.
    Member,
    /// `/org` — an operator of at least one organization.
    Org,
    /// `/platform` — a platform operator.
    Platform,
}

/// What kind of party a reference points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyKind {
    /// The human.
    Person,
    /// A tenant and ownership boundary.
    Organization,
    /// A set of parties; granted, never owning.
    Group,
    /// A machine principal.
    Service,
}

/// Role on an organization or a group. Nesting (`member < admin < owner`) is
/// applied in the kernel, in code; the wire only names the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    /// Belongs.
    Member,
    /// Belongs and administers membership.
    Admin,
    /// Belongs, administers, and is the party ownership transfers from.
    Owner,
}

/// What a `Share` can name. `viewer < commenter < editor` nests on a
/// document; `contact` nests with nothing and is admitted by exactly one kind.
///
/// One enum rather than one per kind, because a share is one command: the
/// *kind* decides which of these words it accepts, and the kernel refuses the
/// rest. A document does not accept `contact`, and a person accepts nothing
/// else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocRole {
    /// May read.
    Viewer,
    /// May read and comment.
    Commenter,
    /// May read, comment and edit.
    Editor,
    /// May see this person's contact details. Only a person admits it, and
    /// only a group may be granted it — which is the whole of contact scoping.
    Contact,
}

/// Any party, named the way a UI names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyRef {
    /// Which sort of party this is.
    pub kind: PartyKind,
    /// Its public id.
    pub public_id: PublicId,
    /// Its display name.
    pub display: String,
}

/// The signed-in registration. A session binds an identity, not a person, so
/// this is what a revocation acts on and what product rows reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityRef {
    /// The identity's public id.
    pub public_id: PublicId,
    /// What to call this registration.
    ///
    /// The person's display name when the identity has resolved to one, which
    /// is every registration this deployment mints. Otherwise the local part
    /// of its address, masked — `r…t@` — which is enough to recognise your own
    /// registration and not enough to read it back.
    ///
    /// Never the identity's `source`. A source names the deployment or the
    /// issuer a registration came from, so using it as a label says the same
    /// word to everybody and tells nobody who they are.
    pub display: String,
}

/// The human the identity resolved to, absent while it is still unresolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonRef {
    /// The person's public id.
    pub public_id: PublicId,
    /// The person's display name.
    pub display: String,
}

/// An organization the principal belongs to, as the chrome lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationRef {
    /// The organization's public id.
    pub public_id: PublicId,
    /// The organization's display name.
    pub display: String,
    /// The principal's role in it.
    pub role: MemberRole,
}

/// The whoami body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Whoami {
    /// The registration this session authenticated.
    pub identity: IdentityRef,
    /// The human it resolved to, if it has been resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<PersonRef>,
    /// Who the principal is acting as: the person, or an organization.
    pub acting_as: PartyRef,
    /// Which bundles this principal may hold.
    pub tiers: Vec<Tier>,
    /// The organizations the chrome offers to act as.
    pub organizations: Vec<OrganizationRef>,
    /// When the session stops working, unix seconds.
    pub session_expires_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::round_trip;

    fn whoami() -> Whoami {
        Whoami {
            identity: IdentityRef {
                public_id: crate::testing::id("i_"),
                // Resolved, so the identity is named by its person.
                display: "Ronit".into(),
            },
            person: Some(PersonRef {
                public_id: crate::testing::id("p_"),
                display: "Ronit".into(),
            }),
            acting_as: PartyRef {
                kind: PartyKind::Organization,
                public_id: crate::testing::id("o_"),
                display: "Isoastra".into(),
            },
            tiers: vec![Tier::Member, Tier::Org, Tier::Platform],
            organizations: vec![OrganizationRef {
                public_id: crate::testing::id("o_"),
                display: "Isoastra".into(),
                role: MemberRole::Owner,
            }],
            session_expires_at: 1_787_000_000,
        }
    }

    #[test]
    fn whoami_round_trips() {
        round_trip(&whoami());
    }

    #[test]
    fn an_unresolved_identity_omits_the_person_entirely_and_is_named_by_a_mask() {
        let mut w = whoami();
        w.person = None;
        w.identity.display = "r…t@".into();
        let json = serde_json::to_value(&w).expect("serialises");
        assert!(json.get("person").is_none());
        assert_eq!(json["identity"]["display"], "r…t@");
        round_trip(&w);
    }

    #[test]
    fn tiers_and_roles_spell_themselves_out_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&Tier::Platform).unwrap(),
            "\"platform\""
        );
        assert_eq!(
            serde_json::to_string(&MemberRole::Owner).unwrap(),
            "\"owner\""
        );
        assert_eq!(
            serde_json::to_string(&DocRole::Commenter).unwrap(),
            "\"commenter\""
        );
        assert_eq!(
            serde_json::to_string(&PartyKind::Service).unwrap(),
            "\"service\""
        );
        round_trip(&DocRole::Viewer);
    }
}
