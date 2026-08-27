//! Registrations, deployment-wide, and what one registration holds.
//!
//! An identity is a registration, not a human, so what this reports is what
//! the registration *is*: where it came from, which zone it lives in, whether
//! it has resolved to a person, and which factors it has proven.
//!
//! What it never reports is a factor's `value`. That column is an argon2 PHC
//! string for a password and an address for an email, and an operator console
//! that printed either would be the enumeration endpoint this whole model
//! exists to not have. An email factor is described by the same mask `whoami`
//! uses — `r…t@`, first and last of the local part — which is enough to match
//! a support call against a row and not enough to write to it. One rule, one
//! implementation ([`crate::api::whoami::name::masked`]).

use std::collections::BTreeMap;

use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey, Identity, Person, Session};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::{Value, json};

use super::platform::{page, subject};
use super::{Params, Row};
use crate::api::whoami::name::masked;

pub(super) const IDENTITIES: &str = "SELECT i.id, i.source, i.home_zone, i.person_id, i.status, \
                                 i.created_at, p.display_name AS person_display \
                          FROM identity i LEFT JOIN party p ON p.id = i.person_id \
                          ORDER BY i.id DESC LIMIT $1";

pub(super) const ONE: &str = "SELECT i.id, i.source, i.home_zone, i.person_id, i.status, \
                          i.created_at, p.display_name AS person_display \
                   FROM identity i LEFT JOIN party p ON p.id = i.person_id WHERE i.id = $1";

/// Every factor of the identities the list returned, in one statement rather
/// than one per row.
///
/// No `ORDER BY`: the rows are gathered into a map keyed by identity, so
/// ordering them first would be a sort over every factor on the deployment to
/// produce an order the map immediately discards.
pub(super) const FACTORS_OF_PAGE: &str = "SELECT f.identity_id, f.id, f.kind, f.verified_at, f.value FROM factor f \
     WHERE f.identity_id IN (SELECT id FROM identity ORDER BY id DESC LIMIT $1)";

pub(super) const FACTORS_OF_ONE: &str = "SELECT f.identity_id, f.id, f.kind, f.verified_at, f.value \
                              FROM factor f WHERE f.identity_id = $1 ORDER BY f.id";

pub(super) const SESSIONS: &str = "SELECT s.id, s.acting_as, s.created_at, s.last_seen_at, s.expires_at, \
                               a.kind AS acting_kind, a.display_name AS acting_display \
                        FROM session s JOIN party a ON a.id = s.acting_as \
                        WHERE s.identity_id = $1 ORDER BY s.id DESC";

/// Both sides of the pair, because a candidate names whichever identity was
/// scanned and this panel is opened from either.
pub(super) const CANDIDATES: &str = "SELECT id, identity_a, identity_b, signal, score, status, created_at \
                          FROM match_candidate \
                          WHERE identity_a = $1 OR identity_b = $1 ORDER BY id DESC";

struct IdentityRow {
    id: Id<Identity>,
    source: String,
    home_zone: String,
    person_id: Option<Id<Person>>,
    person_display: Option<String>,
    status: String,
    created_at: Timestamp,
}

impl FromRow for IdentityRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            source: row.text("source")?,
            home_zone: row.text("home_zone")?,
            person_id: row.id_opt("person_id")?,
            person_display: row.text_opt("person_display")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
        })
    }
}

impl IdentityRow {
    fn head(&self, key: &IdKey, factors: Vec<Value>) -> Value {
        json!({
            "public_id": self.id.public(key),
            "source": self.source,
            "home_zone": self.home_zone,
            "status": self.status,
            "created_at": self.created_at,
            "person": self.person_id.map(|person| json!({
                "public_id": person.public(key),
                "display": self.person_display.clone().unwrap_or_default(),
            })),
            "factors": factors,
        })
    }
}

struct FactorRow {
    identity_id: i64,
    id: i64,
    kind: String,
    verified: bool,
    value: String,
}

impl FromRow for FactorRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            identity_id: row.int("identity_id")?,
            id: row.int("id")?,
            kind: row.text("kind")?,
            verified: row.int_opt("verified_at")?.is_some(),
            value: row.text("value")?,
        })
    }
}

impl FactorRow {
    /// What a factor is allowed to say about itself.
    ///
    /// The `hint` is the masked local part for an address and nothing at all
    /// for anything else: a password's value is a hash, a passkey's is a
    /// credential, and neither has a half that is safe to show.
    fn shown(&self, key: &IdKey) -> Value {
        let hint = (self.kind == "email").then(|| masked(Some(&self.value)));
        json!({
            "public_id": Id::<rn_kernel::ids::Factor>::new(self.id).public(key),
            "kind": self.kind,
            "verified": self.verified,
            "hint": hint,
        })
    }
}

pub(super) async fn list(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    let limit = page(params);
    let mut factors: BTreeMap<i64, Vec<Value>> = BTreeMap::new();
    for factor in reads
        .query::<FactorRow>(FACTORS_OF_PAGE, bind![limit])
        .await?
    {
        factors
            .entry(factor.identity_id)
            .or_default()
            .push(factor.shown(key));
    }
    Ok(reads
        .query::<IdentityRow>(IDENTITIES, bind![limit])
        .await?
        .into_iter()
        .map(|row| Row {
            key: row.id.public(key).as_str().to_owned(),
            value: {
                let held = factors.get(&row.id.get()).cloned().unwrap_or_default();
                row.head(key, held)
            },
        })
        .collect())
}

pub(super) async fn one(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    let Some(identity) = subject::<Identity>(key, params) else {
        return Ok(Vec::new());
    };
    let Some(row) = reads.query_opt::<IdentityRow>(ONE, bind![identity]).await? else {
        return Ok(Vec::new());
    };
    let factors = reads
        .query::<FactorRow>(FACTORS_OF_ONE, bind![identity])
        .await?
        .iter()
        .map(|factor| factor.shown(key))
        .collect();
    let mut value = row.head(key, factors);
    let object = value.as_object_mut().expect("head builds an object");
    object.insert("sessions".to_owned(), sessions(reads, identity, key).await?);
    object.insert(
        "candidates".to_owned(),
        candidates(reads, identity, key).await?,
    );
    Ok(vec![Row {
        key: row.id.public(key).as_str().to_owned(),
        value,
    }])
}

struct SessionRow {
    id: Id<Session>,
    acting_as: i64,
    acting_kind: String,
    acting_display: String,
    created_at: Timestamp,
    last_seen_at: Timestamp,
    expires_at: Timestamp,
}

impl FromRow for SessionRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            acting_as: row.int("acting_as")?,
            acting_kind: row.text("acting_kind")?,
            acting_display: row.text("acting_display")?,
            created_at: row.int("created_at")?,
            last_seen_at: row.int("last_seen_at")?,
            expires_at: row.int("expires_at")?,
        })
    }
}

impl SessionRow {
    pub(super) fn shown(&self, key: &IdKey) -> Value {
        json!({
            "public_id": self.id.public(key),
            "acting_as": super::platform_parties::object_id(&self.acting_kind, self.acting_as, key),
            "acting_kind": self.acting_kind,
            "acting_display": self.acting_display,
            "created_at": self.created_at,
            "last_seen_at": self.last_seen_at,
            "expires_at": self.expires_at,
        })
    }
}

async fn sessions(reads: &impl Reads, identity: Id<Identity>, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<SessionRow>(SESSIONS, bind![identity])
            .await?
            .iter()
            .map(|row| row.shown(key))
            .collect(),
    ))
}

async fn candidates(reads: &impl Reads, identity: Id<Identity>, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<super::platform_matches::CandidateRow>(CANDIDATES, bind![identity])
            .await?
            .iter()
            .map(|row| row.shown(key))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> IdKey {
        IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key")
    }

    fn factor(kind: &str, value: &str) -> FactorRow {
        FactorRow {
            identity_id: 1,
            id: 2,
            kind: kind.to_owned(),
            verified: true,
            value: value.to_owned(),
        }
    }

    #[test]
    fn a_factor_never_carries_its_value_and_an_address_is_only_ever_masked() {
        let key = key();
        let email = factor("email", "ronit@isoastra.com").shown(&key);
        assert_eq!(email["hint"], "r…t@");
        let rendered = email.to_string();
        assert!(!rendered.contains("isoastra"), "{rendered}");
        assert!(!rendered.contains("ronit@"), "{rendered}");

        // A password's value is a hash and has no half worth showing.
        let password = factor("password", "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA");
        let rendered = password.shown(&key).to_string();
        assert_eq!(password.shown(&key)["hint"], Value::Null);
        assert!(!rendered.contains("argon2"), "{rendered}");
    }

    #[test]
    fn a_factor_is_named_by_a_derived_id_rather_than_by_its_row() {
        let key = key();
        let shown = factor("email", "a@b.test").shown(&key);
        let id = shown["public_id"].as_str().expect("an id");
        assert!(id.starts_with("f_"), "{id}");
        assert_eq!(id.len(), 24);
    }
}
