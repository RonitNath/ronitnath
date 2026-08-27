//! The rest of what the deployment knows about one person.
//!
//! `platform-party` already answered four questions — which registrations
//! resolve to it, which containers it belongs to, what it owns, what it has
//! been granted. Story 7 says *everything the system knows*, and five things
//! were missing: the factors those registrations have proven, the sessions
//! they are holding, the clients this person has consented to, the ids and
//! handles a merge absorbed, and the merge history itself.
//!
//! ## Addresses
//!
//! A factor's `value` is an argon2 PHC string for a password and an address
//! for an email, and neither is ever selected into an answer. An address is
//! described by the same mask `whoami` uses — `r…t@`, first and last of the
//! local part — because an operator page that printed every email in the
//! deployment is a page that leaks the deployment the moment somebody
//! screenshots it. One rule, one implementation
//! ([`crate::api::whoami::name::masked`]), and a server test asserts the
//! rendered person carries no `@`-bearing address at all.
//!
//! ## Merge history
//!
//! `person_link` is append-only, so this is the whole of it: every ruling that
//! ever attached a registration to this person, with the method it was made
//! under and the evidence the operator had to state. That is what makes a
//! merge reviewable a year later, and it is why `Split` can undo one.

use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey, Identity, OidcClient, Person, Session};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::{Value, json};

use crate::api::whoami::name::masked;

/// Every factor of every registration this person holds.
///
/// `identity_person_idx` finds the registrations, `factor_identity_idx` finds
/// their factors: two seeks, and a person has a handful of each.
pub(super) const FACTORS: &str = "SELECT f.identity_id, f.id, f.kind, f.verified_at, f.value \
     FROM identity i JOIN factor f ON f.identity_id = i.id \
     WHERE i.person_id = $1 ORDER BY f.id";

/// Every session those registrations are holding, newest first.
pub(super) const SESSIONS: &str = "SELECT s.id, s.identity_id, s.acting_as, s.created_at, \
       s.last_seen_at, s.expires_at, s.impersonated_by_identity_id, \
       a.kind AS acting_kind, a.display_name AS acting_display \
     FROM identity i JOIN session s ON s.identity_id = i.id \
     JOIN party a ON a.id = s.acting_as \
     WHERE i.person_id = $1 ORDER BY s.id DESC";

/// Every client this person has said yes to. `oidc_consent_person_idx`.
pub(super) const CONSENTS: &str = "SELECT k.client_id, k.scopes, k.at, c.client_name \
     FROM oidc_consent k JOIN oidc_client c ON c.id = k.client_id \
     WHERE k.person_id = $1 ORDER BY k.client_id";

/// Every ruling that attached a registration to this person, newest first.
/// `person_link_person_idx`.
pub(super) const MERGES: &str = "SELECT l.identity_id, l.method, l.evidence, l.asserted_by, l.at, \
       p.display_name AS asserted_display \
     FROM person_link l \
     LEFT JOIN identity ai ON ai.id = l.asserted_by \
     LEFT JOIN party p ON p.id = ai.person_id \
     WHERE l.person_id = $1 ORDER BY l.at DESC, l.id DESC";

/// The person ids a merge absorbed, which still resolve here.
/// `person_alias_person_idx`.
pub(super) const ALIASES: &str =
    "SELECT old_person_id, at FROM person_alias WHERE person_id = $1 ORDER BY at DESC";

/// The handles a merge absorbed. Nobody may take one again, and every one of
/// them is a URL that used to name somebody. `party_handle_alias_person_idx`.
pub(super) const HANDLE_ALIASES: &str =
    "SELECT handle, at FROM party_handle_alias WHERE person_id = $1 ORDER BY at DESC";

struct FactorRow {
    identity_id: Id<Identity>,
    id: i64,
    kind: String,
    verified: bool,
    value: String,
}

impl FromRow for FactorRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            identity_id: row.id("identity_id")?,
            id: row.int("id")?,
            kind: row.text("kind")?,
            verified: row.int_opt("verified_at")?.is_some(),
            value: row.text("value")?,
        })
    }
}

/// What a factor is allowed to say about itself.
///
/// The `hint` is the masked local part for an address and nothing at all for
/// anything else: a password's value is a hash and a passkey's is a
/// credential, and neither has a half that is safe to show.
async fn factors(reads: &impl Reads, person: Id<Person>, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<FactorRow>(FACTORS, bind![person])
            .await?
            .into_iter()
            .map(|row| {
                let hint = (row.kind == "email").then(|| masked(Some(&row.value)));
                json!({
                    "public_id": Id::<rn_kernel::ids::Factor>::new(row.id).public(key),
                    "identity": row.identity_id.public(key),
                    "kind": row.kind,
                    "verified": row.verified,
                    "hint": hint,
                })
            })
            .collect(),
    ))
}

struct SessionRow {
    id: Id<Session>,
    identity_id: Id<Identity>,
    acting_as: i64,
    acting_kind: String,
    acting_display: String,
    impersonated_by: Option<Id<Identity>>,
    created_at: Timestamp,
    last_seen_at: Timestamp,
    expires_at: Timestamp,
}

impl FromRow for SessionRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            identity_id: row.id("identity_id")?,
            acting_as: row.int("acting_as")?,
            acting_kind: row.text("acting_kind")?,
            acting_display: row.text("acting_display")?,
            impersonated_by: row.id_opt("impersonated_by_identity_id")?,
            created_at: row.int("created_at")?,
            last_seen_at: row.int("last_seen_at")?,
            expires_at: row.int("expires_at")?,
        })
    }
}

async fn sessions(reads: &impl Reads, person: Id<Person>, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<SessionRow>(SESSIONS, bind![person])
            .await?
            .into_iter()
            .map(|row| {
                json!({
                    "public_id": row.id.public(key),
                    "identity": row.identity_id.public(key),
                    "acting_as": super::platform_parties::object_id(
                        &row.acting_kind, row.acting_as, key,
                    ),
                    "acting_kind": row.acting_kind,
                    "acting_display": row.acting_display,
                    // Set only on a session an operator is wearing. The row is
                    // otherwise an ordinary one, which is the whole design.
                    "impersonated_by": row.impersonated_by.map(|id| id.public(key)),
                    "created_at": row.created_at,
                    "last_seen_at": row.last_seen_at,
                    "expires_at": row.expires_at,
                })
            })
            .collect(),
    ))
}

struct ConsentRow {
    client_id: Id<OidcClient>,
    client_name: String,
    scopes: String,
    at: Timestamp,
}

impl FromRow for ConsentRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            client_id: row.id("client_id")?,
            client_name: row.text("client_name")?,
            scopes: row.text("scopes")?,
            at: row.int("at")?,
        })
    }
}

async fn consents(reads: &impl Reads, person: Id<Person>, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<ConsentRow>(CONSENTS, bind![person])
            .await?
            .into_iter()
            .map(|row| {
                json!({
                    "client": row.client_id.public(key),
                    "client_name": row.client_name,
                    "scopes": row.scopes,
                    "at": row.at,
                })
            })
            .collect(),
    ))
}

struct MergeRow {
    identity_id: Id<Identity>,
    method: String,
    evidence: Option<String>,
    asserted_by: Option<Id<Identity>>,
    asserted_display: Option<String>,
    at: Timestamp,
}

impl FromRow for MergeRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            identity_id: row.id("identity_id")?,
            method: row.text("method")?,
            evidence: row.text_opt("evidence")?,
            asserted_by: row.id_opt("asserted_by")?,
            asserted_display: row.text_opt("asserted_display")?,
            at: row.int("at")?,
        })
    }
}

async fn merges(reads: &impl Reads, person: Id<Person>, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<MergeRow>(MERGES, bind![person])
            .await?
            .into_iter()
            .map(|row| {
                json!({
                    "identity": row.identity_id.public(key),
                    "method": row.method,
                    // What the operator relied on. Empty for a self-link,
                    // which is a person confirming their own registration and
                    // has nobody to state a reason to.
                    "evidence": row.evidence,
                    "asserted_by": row.asserted_by.map(|id| id.public(key)),
                    "asserted_display": row.asserted_display,
                    "at": row.at,
                })
            })
            .collect(),
    ))
}

struct AliasRow {
    old_person_id: Id<Person>,
    at: Timestamp,
}

impl FromRow for AliasRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            old_person_id: row.id("old_person_id")?,
            at: row.int("at")?,
        })
    }
}

struct HandleAliasRow {
    handle: String,
    at: Timestamp,
}

impl FromRow for HandleAliasRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            handle: row.text("handle")?,
            at: row.int("at")?,
        })
    }
}

/// Both kinds of absorbed name, in one list: an id that still resolves here,
/// and a handle nobody may take again.
async fn aliases(reads: &impl Reads, person: Id<Person>, key: &IdKey) -> Outcome<Value> {
    let mut all: Vec<Value> = reads
        .query::<AliasRow>(ALIASES, bind![person])
        .await?
        .into_iter()
        .map(|row| {
            json!({
                "kind": "id",
                "was": row.old_person_id.public(key).as_str().to_owned(),
                "at": row.at,
            })
        })
        .collect();
    all.extend(
        reads
            .query::<HandleAliasRow>(HANDLE_ALIASES, bind![person])
            .await?
            .into_iter()
            .map(|row| json!({ "kind": "handle", "was": row.handle, "at": row.at })),
    );
    Ok(Value::Array(all))
}

/// Add the five to a party's drill-in, when the party is a person.
///
/// An organization, a group and a service have none of them: they have no
/// registrations, so no factors and no sessions, and nothing merges them.
pub(super) async fn extend(
    reads: &impl Reads,
    object: &mut serde_json::Map<String, Value>,
    person: Id<Person>,
    key: &IdKey,
) -> Outcome<()> {
    object.insert("factors".to_owned(), factors(reads, person, key).await?);
    object.insert("sessions".to_owned(), sessions(reads, person, key).await?);
    object.insert("consents".to_owned(), consents(reads, person, key).await?);
    object.insert("merges".to_owned(), merges(reads, person, key).await?);
    object.insert("aliases".to_owned(), aliases(reads, person, key).await?);
    Ok(())
}
