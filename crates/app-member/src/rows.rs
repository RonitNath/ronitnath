//! The rows the member queries return, as this bundle reads them.
//!
//! One struct per `/api/q/<name>`, with the field names the server wrote. They
//! are deliberately not shared with the server: the wire is
//! `docs/rebuild/plan.md` §API and these are a reader of it, so a field the
//! server stops sending shows up here as a deserialisation failure on the page
//! that needed it rather than as a type error in a crate nobody edited.
//!
//! Every id is a string, because a public id is a string on the wire and this
//! bundle never does anything with one but send it back.

use serde::Deserialize;

/// A party as a row names it — the owner of a document, the container of an
/// invitation, a member of a group.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PartyRef {
    #[serde(default)]
    pub public_id: Option<String>,
    pub kind: String,
    pub display: String,
}

/// `sessions` — one device.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Session {
    pub public_id: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub last_seen_at: i64,
    pub current: bool,
}

/// `identities` — one registration and what it has proven.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Identity {
    pub public_id: String,
    pub source: String,
    pub home_zone: String,
    pub status: String,
    pub created_at: i64,
    pub current: bool,
    pub factors: Vec<Factor>,
}

/// One factor of a registration. Its value never crosses the wire.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Factor {
    pub public_id: String,
    pub kind: String,
    pub verified: bool,
}

/// `audit` — one of my own commands.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Audited {
    pub offset: u64,
    pub command: String,
    pub at: i64,
}

/// `groups` — a group I belong to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Group {
    pub public_id: String,
    pub display: String,
    pub role: String,
    pub joined_at: i64,
    pub owner: PartyRef,
}

/// `group-members` — somebody in one of those groups.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Member {
    pub group: String,
    #[serde(default)]
    pub public_id: Option<String>,
    pub kind: String,
    pub display: String,
    pub role: String,
    pub joined_at: i64,
    pub you: bool,
    pub contact: bool,
}

/// `invitations` — a link I minted.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Invitation {
    pub created_at: i64,
    pub expires_at: i64,
    #[serde(default)]
    pub claimed_at: Option<i64>,
    #[serde(default)]
    pub claimed_by: Option<String>,
    pub role: String,
    pub container: PartyRef,
}

/// `documents` — a document I own or hold a relation on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Document {
    pub public_id: String,
    pub title: String,
    pub status: String,
    pub created_at: i64,
    pub draft_rev: i64,
    #[serde(default)]
    pub published_rev: Option<i64>,
    pub unpublished: bool,
    pub mine: bool,
    pub relation: String,
    pub may_edit: bool,
    pub owner: PartyRef,
    /// Only the single-document read carries the words.
    #[serde(default)]
    pub body: Option<String>,
}

/// `shares` — one grant on one of those documents.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Share {
    pub document: String,
    pub subject: String,
    pub kind: String,
    pub display: String,
    pub relation: String,
    pub at: i64,
}

/// `matches` — a pair a signal proposed about one of my registrations.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Candidate {
    pub public_id: String,
    pub signal: String,
    pub score: f64,
    pub created_at: i64,
    pub mine: String,
    pub other: String,
    #[serde(default)]
    pub display: Option<String>,
    pub both: bool,
}
