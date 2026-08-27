//! The resource registry: sharing, ownership transfer, and documents — the
//! first registered resource kind.

use serde::{Deserialize, Serialize};

use crate::ids::PublicId;
use crate::whoami::DocRole;

/// Grant a relation on a resource to a subject.
///
/// The subject's public id carries its kind, so one command covers sharing
/// with a person, an organization, a group or an invitation link — and which
/// of those a resource kind will accept is the kind's own vocabulary, checked
/// in the kernel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    /// The resource being shared.
    pub resource: PublicId,
    /// Who it is shared with.
    pub subject: PublicId,
    /// What they may do.
    pub relation: DocRole,
}

/// Withdraw a relation. Exactly the row [`Share`] wrote, no more: a subject
/// that also reaches the resource through a group keeps that path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revoke {
    /// The resource.
    pub resource: PublicId,
    /// The subject losing the relation.
    pub subject: PublicId,
    /// The relation being withdrawn.
    pub relation: DocRole,
}

/// Move a resource's ownership. The only way `owner_party_id` ever changes,
/// and it refuses a group: groups are granted, never owners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    /// The resource changing hands.
    pub resource: PublicId,
    /// The person or organization receiving it.
    pub to: PublicId,
}

/// Create a document, in `draft`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateDocument {
    /// The document's title.
    pub title: String,
    /// Its body.
    pub body: String,
    /// The organization to own it. Absent means the acting person owns it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<PublicId>,
}

/// Edit a document. Both fields are optional so a title change does not have
/// to resend the body; sending neither is a no-op the kernel refuses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditDocument {
    /// The document being edited.
    pub document: PublicId,
    /// A new title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A new body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// Move a document from `draft` to `published`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishDocument {
    /// The document to publish.
    pub document: PublicId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{id, round_trip};

    #[test]
    fn resource_commands_round_trip() {
        round_trip(&Share {
            resource: id("r_"),
            subject: id("g_"),
            relation: DocRole::Commenter,
        });
        round_trip(&Revoke {
            resource: id("r_"),
            subject: id("p_"),
            relation: DocRole::Editor,
        });
        round_trip(&Transfer {
            resource: id("r_"),
            to: id("o_"),
        });
        round_trip(&CreateDocument {
            title: "Kernel report".into(),
            body: "Five rules".into(),
            owner: Some(id("o_")),
        });
        round_trip(&PublishDocument { document: id("r_") });
    }

    #[test]
    fn an_edit_sends_only_what_changed() {
        let edit = EditDocument {
            document: id("r_"),
            title: Some("Kernel report, revised".into()),
            body: None,
        };
        let json = serde_json::to_value(&edit).expect("serialises");
        assert!(json.get("body").is_none());
        round_trip(&edit);
    }
}
