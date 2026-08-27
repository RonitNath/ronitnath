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
    /// A person's `preferred_username`. Absent on every other kind.
    pub handle: Option<String>,
    pub status: String,
    pub created_at: i64,
}

/// A party's drill-in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PartyDetail {
    pub public_id: PublicId,
    pub kind: String,
    pub display: String,
    pub handle: Option<String>,
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

/// One node's reading of itself, and the verdict counted from the whole set.
///
/// `rollout` and `versions` are the same on every row because every row
/// counted the same set; the screen reads them once.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Node {
    pub node_id: i64,
    pub node: String,
    pub version: String,
    pub raft_role: String,
    pub feed_head: i64,
    pub reported_at: i64,
    pub reported_ago: i64,
    /// Whether the last reading is fresh enough to be current. False is what
    /// the screen says as *not reporting*.
    pub reporting: bool,
    pub rollout: String,
    pub versions: Vec<String>,
}

/// The two feed readings. Never a rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Feed {
    pub offset: i64,
    pub commands_today: i64,
}

/// A signing key, and what retiring it would make unverifiable.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Key {
    pub kid: String,
    pub status: String,
    pub created_at: i64,
    pub age_days: i64,
    pub retired_at: Option<i64>,
    pub signed_tokens_alive: i64,
}

/// A registered relying party.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Client {
    pub public_id: PublicId,
    pub client_name: String,
    pub client_uri: Option<String>,
    pub owner: Option<PublicId>,
    pub owner_display: Option<String>,
    pub redirect_uris: Vec<String>,
    pub token_endpoint_auth_method: String,
    pub scopes: Vec<String>,
    pub trusted: bool,
    pub members_only: bool,
    pub created_at: i64,
    pub rotated_at: Option<i64>,
    pub withdrawn_at: Option<i64>,
    pub consents: i64,
    pub live_tokens: i64,
    pub last_issued_at: Option<i64>,
}

/// An outstanding invitation. Named by its public id, never by its token.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Invitation {
    pub public_id: PublicId,
    pub container: Option<PublicId>,
    pub container_kind: String,
    pub container_display: Option<String>,
    pub role: String,
    pub minted_by: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub claimed_at: Option<i64>,
    pub claimed_by: Option<String>,
    pub suspended_at: Option<i64>,
    pub state: String,
}

/// One person's consent to one client.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Consent {
    pub client: PublicId,
    pub client_name: String,
    pub person: PublicId,
    pub person_display: String,
    pub handle: Option<String>,
    pub scopes: String,
    pub at: i64,
}

/// What a disable would end, counted at the time of asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Cascade {
    pub sessions: i64,
    pub tokens: i64,
    pub links: i64,
    pub consents: i64,
}

/// A row the one search box found, and which of the three seeks found it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Found {
    pub public_id: PublicId,
    pub kind: String,
    pub display: String,
    pub handle: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub via: String,
}

/// A factor of one of a person's registrations. Never a value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PersonFactor {
    pub public_id: PublicId,
    pub identity: PublicId,
    pub kind: String,
    pub verified: bool,
    /// The masked local part, for an address. Nothing for anything else.
    pub hint: Option<String>,
}

/// A session one of those registrations is holding.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PersonSession {
    pub public_id: PublicId,
    pub identity: PublicId,
    pub acting_kind: String,
    pub acting_display: String,
    /// Set only on a session an operator is wearing.
    pub impersonated_by: Option<PublicId>,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
}

/// A client this person has said yes to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PersonConsent {
    pub client: PublicId,
    pub client_name: String,
    pub scopes: String,
    pub at: i64,
}

/// One ruling that attached a registration to this person.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Merge {
    pub identity: PublicId,
    pub method: String,
    pub evidence: Option<String>,
    pub asserted_by: Option<PublicId>,
    pub asserted_display: Option<String>,
    pub at: i64,
}

/// A name a merge absorbed: an id that still resolves here, or a handle
/// nobody may take again.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AbsorbedName {
    pub kind: String,
    pub was: String,
    pub at: i64,
}

/// The person page: everything the deployment knows about one human.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Person {
    pub public_id: PublicId,
    pub kind: String,
    pub display: String,
    pub handle: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub identities: Vec<PartyIdentity>,
    pub memberships: Vec<Membership>,
    pub resources: Vec<OwnedResource>,
    pub relations: Vec<HeldRelation>,
    #[serde(default)]
    pub factors: Vec<PersonFactor>,
    #[serde(default)]
    pub sessions: Vec<PersonSession>,
    #[serde(default)]
    pub consents: Vec<PersonConsent>,
    #[serde(default)]
    pub merges: Vec<Merge>,
    #[serde(default)]
    pub aliases: Vec<AbsorbedName>,
}

/// Somebody who holds `platform:* #operator`.
///
/// `granted_by` is false on exactly one row — the bootstrap — and the screen
/// says *granted by configuration* there rather than leaving a dash, because
/// a dash is what a page prints when it does not know.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Operator {
    pub public_id: PublicId,
    pub display: String,
    pub handle: Option<String>,
    pub status: String,
    pub at: i64,
    pub granted_by: bool,
    pub granted_display: Option<String>,
}
