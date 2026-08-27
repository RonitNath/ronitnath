//! The shapes `/api/q/platform-*` answers with.
//!
//! Written out rather than read as loose JSON: a column that reads a field
//! nobody serves should not compile, and a server that stops serving one
//! should break a build rather than render an empty cell. Every id here is a
//! [`PublicId`] for the same reason — a row's identifier is validated on the
//! way in, so a command built from one cannot be built from junk.
//!
//! The `Option`s are the model's, not defensiveness: an identity that has not
//! resolved has no person, a relation on `platform` has no object id, and a
//! factor that is not an address has no half worth showing.

use rn_api::PublicId;
use serde::Deserialize;

/// A party, as the list renders it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Party {
    pub public_id: PublicId,
    pub kind: String,
    pub display: String,
    pub status: String,
    pub created_at: i64,
}

/// A party's drill-in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PartyDetail {
    pub public_id: PublicId,
    pub kind: String,
    pub display: String,
    pub status: String,
    pub created_at: i64,
    pub identities: Vec<PartyIdentity>,
    pub memberships: Vec<Membership>,
    pub resources: Vec<OwnedResource>,
    pub relations: Vec<HeldRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PartyIdentity {
    pub public_id: PublicId,
    pub source: String,
    pub home_zone: String,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Membership {
    pub public_id: PublicId,
    pub kind: String,
    pub display: String,
    pub role: String,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OwnedResource {
    pub public_id: PublicId,
    pub kind: String,
    pub status: String,
    pub created_at: i64,
}

/// A relation this party holds, read from the subject side.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HeldRelation {
    pub object_kind: String,
    /// Absent for `platform`, which is the deployment itself and addresses no
    /// row.
    pub object: Option<PublicId>,
    pub relation: String,
    pub at: i64,
}

/// The person a registration resolved to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PersonRef {
    pub public_id: PublicId,
    pub display: String,
}

/// A registration, as the list renders it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Identity {
    pub public_id: PublicId,
    pub source: String,
    pub home_zone: String,
    pub status: String,
    pub created_at: i64,
    pub person: Option<PersonRef>,
    pub factors: Vec<Factor>,
}

/// What a factor is allowed to say about itself. Never a value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Factor {
    pub public_id: PublicId,
    pub kind: String,
    pub verified: bool,
    /// The masked local part, for an address. Nothing for anything else.
    pub hint: Option<String>,
}

/// A registration's drill-in.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IdentityDetail {
    pub public_id: PublicId,
    pub source: String,
    pub home_zone: String,
    pub status: String,
    pub created_at: i64,
    pub person: Option<PersonRef>,
    pub factors: Vec<Factor>,
    pub sessions: Vec<IdentitySession>,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IdentitySession {
    pub public_id: PublicId,
    pub acting_kind: String,
    pub acting_display: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
}

/// A live session, deployment-wide.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Session {
    pub public_id: PublicId,
    pub identity: PublicId,
    /// The person the registration resolved to, if it has.
    pub person: Option<String>,
    pub acting_kind: String,
    pub acting_display: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
}

/// One audit row. The key is the offset, and so is the first column.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Audit {
    pub offset: i64,
    pub command: String,
    pub actor: Option<PublicId>,
    pub actor_display: Option<String>,
    pub acting_as: Option<PublicId>,
    /// The acting party's display name. A name is what a reader recognises;
    /// the id above it is what they quote.
    pub acting_display: Option<String>,
    pub at: i64,
    /// The typed event the command's own transaction wrote.
    pub payload: serde_json::Value,
}

/// A queued pair.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Candidate {
    pub public_id: PublicId,
    pub identity_a: PublicId,
    pub identity_b: PublicId,
    pub signal: String,
    pub score: f64,
    pub status: String,
    pub created_at: i64,
    #[serde(default)]
    pub person_a: Option<PersonRef>,
    #[serde(default)]
    pub person_b: Option<PersonRef>,
}

/// A candidate's drill-in: what a ruling on it produced.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CandidateDetail {
    pub public_id: PublicId,
    pub identity_a: PublicId,
    pub identity_b: PublicId,
    pub signal: String,
    pub score: f64,
    pub status: String,
    pub created_at: i64,
    pub person_a: Option<PersonRef>,
    pub person_b: Option<PersonRef>,
    pub links: Vec<PersonLink>,
    pub alias: Vec<Alias>,
}

/// A `person_link` row. Append-only, so this is the whole history.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PersonLink {
    pub identity: PublicId,
    pub person: PublicId,
    pub from_person: Option<PublicId>,
    pub method: String,
    pub evidence: Option<String>,
    pub asserted_by: Option<PublicId>,
    pub at: i64,
}

/// A `person_alias` row: an id that still resolves to somebody.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Alias {
    pub was: PublicId,
    pub now: PublicId,
    pub at: i64,
}

/// A row in the resource registry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Resource {
    pub public_id: PublicId,
    pub kind: String,
    pub owner: Option<PublicId>,
    pub owner_kind: String,
    pub owner_display: String,
    pub home_zone: String,
    pub status: String,
    pub created_at: i64,
}

/// A resource's drill-in: the relations held on it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ResourceDetail {
    pub public_id: PublicId,
    pub kind: String,
    pub owner: Option<PublicId>,
    pub owner_kind: String,
    pub owner_display: String,
    pub home_zone: String,
    pub status: String,
    pub created_at: i64,
    pub relations: Vec<GrantedRelation>,
}

/// A relation held on a resource, read from the object side.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GrantedRelation {
    pub relation: String,
    pub subject_kind: String,
    /// Absent for `public`, `authenticated` and an invitation link — the three
    /// subjects with no row of their own to name.
    pub subject: Option<PublicId>,
    pub at: i64,
}
