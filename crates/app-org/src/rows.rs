//! The rows the `/org` queries return, as this bundle reads them.
//!
//! One struct per query shape, deserialised from the JSON the server built.
//! They are deliberately plain: an id is the string that was on the wire, a
//! timestamp is unix seconds, and nothing here derives a fact the server did
//! not state — a page that computed "is this the last owner" from a list it
//! happened to be holding would answer differently from the command that
//! refuses it.

use serde::Deserialize;

/// A party as a row names it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Named {
    /// Its public id.
    pub public_id: String,
    /// What to call it.
    pub display: String,
}

/// `org` — the organization itself.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Org {
    /// The party id: what memberships and invitations address.
    pub public_id: String,
    /// The resource id: what `Transfer` addresses.
    pub resource: String,
    /// Its name.
    pub display: String,
    /// `active` or `disabled`.
    pub status: String,
    /// When it was founded, unix seconds.
    pub created_at: i64,
    /// The party that owns it.
    pub owner: Named,
    /// How many parties belong.
    pub members: i64,
    /// How many groups it owns.
    pub groups: i64,
    /// How many documents it owns.
    pub documents: i64,
    /// What the reader holds on it.
    pub my_role: String,
    /// Whether the reader is the owning party themselves — which is the whole
    /// of who may transfer it.
    pub owned_by_me: bool,
}

/// `org-members` — one membership.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Member {
    /// The member's public id.
    pub public_id: String,
    /// Their name.
    pub display: String,
    /// `person`, `organization`, `group` or `service`.
    pub kind: String,
    /// The party's status.
    pub status: String,
    /// `member`, `admin` or `owner`.
    pub role: String,
    /// When they joined, unix seconds.
    pub since: i64,
    /// Whether this row is the reader.
    pub me: bool,
    /// Whether the reader may move this role at all.
    pub settable: bool,
    /// Whether this is the container's only owner, who cannot be demoted.
    pub last_owner: bool,
}

/// `org-groups` — one group of the organization.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Group {
    /// The group's party id.
    pub public_id: String,
    /// Its resource id.
    pub resource: String,
    /// Its name.
    pub display: String,
    /// Its status.
    pub status: String,
    /// When it was created, unix seconds.
    pub created_at: i64,
    /// How many parties belong to it.
    pub members: i64,
}

/// `org-contacts` — a person whose details are scoped into a group.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Contact {
    /// The person's public id.
    pub public_id: String,
    /// Their name.
    pub display: String,
    /// Their status.
    pub status: String,
}

/// One grant on a document.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Grant {
    /// `viewer`, `commenter`, `editor` or `owner`.
    pub relation: String,
    /// What sort of subject holds it.
    pub subject_kind: String,
    /// Its public id, when the subject is a party that has one.
    pub subject: Option<String>,
    /// Its name, when the subject is a party.
    pub display: Option<String>,
}

/// `org-documents` — a document and everything shared on it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Document {
    /// The document's resource id.
    pub public_id: String,
    /// Its title.
    pub title: String,
    /// Its body.
    pub body: String,
    /// `draft` or `published`.
    pub status: String,
    /// When it was created, unix seconds.
    pub created_at: i64,
    /// The revision an edit must be sent against.
    pub draft_rev: i64,
    /// The revision that is published, if any.
    pub published_rev: Option<i64>,
    /// Whether the draft is ahead of what is published.
    pub unpublished: bool,
    /// Every grant made on it.
    pub shared: Vec<Grant>,
}

/// `org-invitations` — one minted link, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Invitation {
    /// The row's key in the result set.
    pub key: String,
    /// The container it joins.
    pub container: Option<String>,
    /// That container's name.
    pub container_display: String,
    /// The role it carries.
    pub role: String,
    /// When it stops working, unix seconds.
    pub expires_at: i64,
    /// When it was minted, unix seconds.
    pub created_at: i64,
    /// When it was claimed, if it was.
    pub claimed_at: Option<i64>,
    /// Who claimed it.
    pub claimed_by: Option<String>,
    /// Who minted it.
    pub minted_by: Option<String>,
    /// Whether it can still be claimed.
    pub live: bool,
}

/// `org-audit` — one command that touched this organization.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Audited {
    /// The change-feed offset.
    pub offset: u64,
    /// The command's route name.
    pub command: String,
    /// When it ran, unix seconds.
    pub at: i64,
    /// Who ran it.
    pub actor: Option<String>,
}
