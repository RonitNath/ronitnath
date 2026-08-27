//! Parties, deployment-wide, and what one party is made of.
//!
//! A party is the thing that can own, be granted, or belong, and the drill-in
//! is the four answers that follow from that: which registrations resolve to
//! it, which containers it belongs to, what it owns, and what it has been
//! granted. Four statements rather than one join, because they are four
//! different questions and a single row of nested arrays that failed halfway
//! would say nothing about which half.

use rn_kernel::bind;
use rn_kernel::domain::{PartyKind, Vocabulary as _};
use rn_kernel::ids::{Id, IdKey, Identity, Person, Resource};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::{Value, json};

use super::platform::{page, subject};
use super::{Params, Row};
use crate::api::party::public_of;

pub(super) const PARTIES: &str = "SELECT id, kind, display_name, handle, status, created_at \
                        FROM party ORDER BY id DESC LIMIT $1";

pub(super) const ONE: &str =
    "SELECT id, kind, display_name, handle, status, created_at FROM party WHERE id = $1";

pub(super) const IDENTITIES: &str = "SELECT id, source, home_zone, status, created_at FROM identity \
                          WHERE person_id = $1 ORDER BY id";

pub(super) const MEMBERSHIPS: &str = "SELECT m.group_id, m.role, m.at, p.kind, p.display_name \
                           FROM membership m JOIN party p ON p.id = m.group_id \
                           WHERE m.party_id = $1 ORDER BY p.display_name";

pub(super) const OWNED: &str = "SELECT id, kind, status, created_at FROM resource \
                     WHERE owner_party_id = $1 ORDER BY id DESC LIMIT $2";

pub(super) const AS_SUBJECT: &str = "SELECT object_kind, object_id, relation, at FROM relation \
                          WHERE subject_key = $1 ORDER BY id";

/// A `party` row, as both the list and the drill-in read it.
pub struct PartyRow {
    id: i64,
    kind: PartyKind,
    display_name: String,
    /// The person's `preferred_username`. NULL on every other kind, and the
    /// list carries it because it is the name an operator is handed on a
    /// support call.
    handle: Option<String>,
    status: String,
    created_at: Timestamp,
}

impl FromRow for PartyRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let raw = row.text("kind")?;
        Ok(Self {
            id: row.int("id")?,
            kind: PartyKind::parse(&raw).ok_or(RowError::Column {
                column: "kind".to_owned(),
                wanted: "PartyKind",
            })?,
            display_name: row.text("display_name")?,
            handle: row.text_opt("handle")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
        })
    }
}

impl PartyRow {
    fn head(&self, key: &IdKey) -> Value {
        json!({
            "public_id": public_of(self.kind, self.id, key),
            "kind": self.kind.as_str(),
            "display": self.display_name,
            "handle": self.handle,
            "status": self.status,
            "created_at": self.created_at,
        })
    }

    /// The `relation.subject_key` this party is addressed by.
    fn subject_key(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.id)
    }
}

pub(super) async fn list(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    Ok(reads
        .query::<PartyRow>(PARTIES, bind![page(params)])
        .await?
        .into_iter()
        .map(|row| Row {
            key: public_of(row.kind, row.id, key).as_str().to_owned(),
            value: row.head(key),
        })
        .collect())
}

pub(super) async fn one(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    // A party's public id carries its kind, and the four kinds share a table:
    // whichever tag the id decodes under is the row it names.
    let Some(id) = any_party(key, params) else {
        return Ok(Vec::new());
    };
    let Some(row) = reads.query_opt::<PartyRow>(ONE, bind![id]).await? else {
        return Ok(Vec::new());
    };
    let mut value = row.head(key);
    let object = value.as_object_mut().expect("head builds an object");
    object.insert("identities".to_owned(), identities(reads, id, key).await?);
    object.insert("memberships".to_owned(), memberships(reads, id, key).await?);
    object.insert("resources".to_owned(), owned(reads, id, key, params).await?);
    object.insert(
        "relations".to_owned(),
        as_subject(reads, &row.subject_key(), key).await?,
    );
    // The five a *person* has and no other kind does: factors, sessions,
    // consents, merge history and absorbed names. An organization has no
    // registrations, so it has none of them, and asking would be four seeks
    // that can only answer nothing.
    if row.kind == PartyKind::Person {
        super::platform_person::extend(reads, object, Id::new(id), key).await?;
    }
    Ok(vec![Row {
        key: public_of(row.kind, row.id, key).as_str().to_owned(),
        value,
    }])
}

/// A party id under whichever of the four tags it was minted with.
///
/// Every party is a `party` rowid, so the *row* is the same integer whichever
/// tag decoded it — but a tag that does not match refuses, which is what stops
/// a person id opening an organization's panel.
fn any_party(key: &IdKey, params: &Params) -> Option<i64> {
    subject::<Person>(key, params)
        .map(Id::get)
        .or_else(|| subject::<rn_kernel::ids::Organization>(key, params).map(Id::get))
        .or_else(|| subject::<rn_kernel::ids::Group>(key, params).map(Id::get))
        .or_else(|| subject::<rn_kernel::ids::Service>(key, params).map(Id::get))
}

struct IdentityRow {
    id: Id<Identity>,
    source: String,
    home_zone: String,
    status: String,
    created_at: Timestamp,
}

impl FromRow for IdentityRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            source: row.text("source")?,
            home_zone: row.text("home_zone")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
        })
    }
}

async fn identities(reads: &impl Reads, party: i64, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<IdentityRow>(IDENTITIES, bind![party])
            .await?
            .into_iter()
            .map(|row| {
                json!({
                    "public_id": row.id.public(key),
                    "source": row.source,
                    "home_zone": row.home_zone,
                    "status": row.status,
                    "created_at": row.created_at,
                })
            })
            .collect(),
    ))
}

struct MembershipRow {
    group_id: i64,
    kind: PartyKind,
    display_name: String,
    role: String,
    at: Timestamp,
}

impl FromRow for MembershipRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let raw = row.text("kind")?;
        Ok(Self {
            group_id: row.int("group_id")?,
            kind: PartyKind::parse(&raw).ok_or(RowError::Column {
                column: "kind".to_owned(),
                wanted: "PartyKind",
            })?,
            display_name: row.text("display_name")?,
            role: row.text("role")?,
            at: row.int("at")?,
        })
    }
}

async fn memberships(reads: &impl Reads, party: i64, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<MembershipRow>(MEMBERSHIPS, bind![party])
            .await?
            .into_iter()
            .map(|row| {
                json!({
                    "public_id": public_of(row.kind, row.group_id, key),
                    "kind": row.kind.as_str(),
                    "display": row.display_name,
                    "role": row.role,
                    "at": row.at,
                })
            })
            .collect(),
    ))
}

struct ResourceRow {
    id: Id<Resource>,
    kind: String,
    status: String,
    created_at: Timestamp,
}

impl FromRow for ResourceRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            kind: row.text("kind")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
        })
    }
}

async fn owned(reads: &impl Reads, party: i64, key: &IdKey, params: &Params) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<ResourceRow>(OWNED, bind![party, page(params)])
            .await?
            .into_iter()
            .map(|row| {
                json!({
                    "public_id": row.id.public(key),
                    "kind": row.kind,
                    "status": row.status,
                    "created_at": row.created_at,
                })
            })
            .collect(),
    ))
}

/// One `relation` row, read from either side.
pub struct RelationRow {
    /// The registered kind of the object.
    pub object_kind: String,
    /// The row the object addresses.
    pub object_id: i64,
    /// What is granted.
    pub relation: String,
    /// When it was granted.
    pub at: Timestamp,
}

impl FromRow for RelationRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            object_kind: row.text("object_kind")?,
            object_id: row.int("object_id")?,
            relation: row.text("relation")?,
            at: row.int("at")?,
        })
    }
}

/// The public id of a relation's object, when the kind is one that has one.
///
/// `platform` is the deployment itself and addresses row zero, so it has no id
/// and is named by its kind alone — an id derived for it would decode to a row
/// that does not exist.
#[must_use]
pub fn object_id(kind: &str, id: i64, key: &IdKey) -> Value {
    match kind {
        "person" => json!(Id::<Person>::new(id).public(key)),
        "organization" => json!(Id::<rn_kernel::ids::Organization>::new(id).public(key)),
        "group" => json!(Id::<rn_kernel::ids::Group>::new(id).public(key)),
        "platform" => Value::Null,
        // Every remaining registered kind is resource-backed.
        _ => json!(Id::<Resource>::new(id).public(key)),
    }
}

async fn as_subject(reads: &impl Reads, subject_key: &str, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<RelationRow>(AS_SUBJECT, bind![subject_key])
            .await?
            .into_iter()
            .map(|row| {
                json!({
                    "object_kind": row.object_kind,
                    "object": object_id(&row.object_kind, row.object_id, key),
                    "relation": row.relation,
                    "at": row.at,
                })
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> IdKey {
        IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key")
    }

    #[test]
    fn the_deployment_itself_is_named_by_its_kind_and_carries_no_id() {
        assert_eq!(object_id("platform", 0, &key()), Value::Null);
    }

    #[test]
    fn an_objects_id_is_derived_under_the_tag_its_kind_carries() {
        let key = key();
        let ids: Vec<String> = ["person", "organization", "group", "document"]
            .iter()
            .map(|kind| object_id(kind, 7, &key).as_str().expect("an id").to_owned())
            .collect();
        assert!(ids[0].starts_with("p_"));
        assert!(ids[1].starts_with("o_"));
        assert!(ids[2].starts_with("g_"));
        assert!(ids[3].starts_with("r_"));
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "two kinds share an id: {ids:?}");
    }

    #[test]
    fn a_party_is_addressed_by_the_key_the_relation_index_is_built_on() {
        let row = PartyRow {
            id: 3,
            kind: PartyKind::Organization,
            display_name: "Isoastra".to_owned(),
            handle: None,
            status: "active".to_owned(),
            created_at: 0,
        };
        assert_eq!(row.subject_key(), "organization:3");
    }
}
