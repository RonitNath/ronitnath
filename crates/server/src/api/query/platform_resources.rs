//! The resource registry, and the relations held on one resource.
//!
//! Every ownable thing has a row here — a document, an organization, a group —
//! and the kind tables hang off it 1:1. So this is the one list from which
//! "who owns what" is answerable without asking each product, which is exactly
//! what the registry is for.
//!
//! The drill-in reads the relation store from the *object* side, which is the
//! opposite direction to `check()`. That is deliberate and it is why this
//! query is bounded: `relation_object_idx` covers the seek, and the number of
//! grants on one object is chosen by whoever shared it rather than by the
//! reader.
//!
//! ## Which object a resource is
//!
//! A document is addressed by its `resource` row. An organization and a group
//! are addressed by their *party* row — memberships and relations point there,
//! not at the resource that owns them — so the drill-in follows
//! `party_resource` before it reads a single relation. Reading the wrong side
//! would return an empty list and call it "no grants".

use rn_kernel::bind;
use rn_kernel::ids::{Id, IdKey, Resource};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::{Value, json};

use super::platform::{page, subject};
use super::platform_parties::object_id;
use super::{Params, Row};

pub(super) const RESOURCES: &str = "SELECT r.id, r.kind, r.owner_party_id, r.home_zone, r.status, \
                                r.created_at, o.kind AS owner_kind, \
                                o.display_name AS owner_display \
                         FROM resource r JOIN party o ON o.id = r.owner_party_id \
                         ORDER BY r.id DESC LIMIT $1";

pub(super) const ONE: &str = "SELECT r.id, r.kind, r.owner_party_id, r.home_zone, r.status, r.created_at, \
                          o.kind AS owner_kind, o.display_name AS owner_display \
                   FROM resource r JOIN party o ON o.id = r.owner_party_id WHERE r.id = $1";

/// The party a party-backed resource stands for.
pub(super) const PARTY_OF: &str = "SELECT party_id AS n FROM party_resource WHERE resource_id = $1";

pub(super) const AS_OBJECT: &str = "SELECT relation, subject_kind, subject_id, at FROM relation \
                         WHERE object_kind = $1 AND object_id = $2 ORDER BY id";

struct Registered {
    id: Id<Resource>,
    kind: String,
    owner_party_id: i64,
    owner_kind: String,
    owner_display: String,
    home_zone: String,
    status: String,
    created_at: Timestamp,
}

impl FromRow for Registered {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            kind: row.text("kind")?,
            owner_party_id: row.int("owner_party_id")?,
            owner_kind: row.text("owner_kind")?,
            owner_display: row.text("owner_display")?,
            home_zone: row.text("home_zone")?,
            status: row.text("status")?,
            created_at: row.int("created_at")?,
        })
    }
}

impl Registered {
    fn head(&self, key: &IdKey) -> Value {
        json!({
            "public_id": self.id.public(key),
            "kind": self.kind,
            "owner": object_id(&self.owner_kind, self.owner_party_id, key),
            "owner_kind": self.owner_kind,
            "owner_display": self.owner_display,
            "home_zone": self.home_zone,
            "status": self.status,
            "created_at": self.created_at,
        })
    }
}

pub(super) async fn list(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    Ok(reads
        .query::<Registered>(RESOURCES, bind![page(params)])
        .await?
        .into_iter()
        .map(|row| Row {
            key: row.id.public(key).as_str().to_owned(),
            value: row.head(key),
        })
        .collect())
}

pub(super) async fn one(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key = reads.ids();
    let Some(resource) = subject::<Resource>(key, params) else {
        return Ok(Vec::new());
    };
    let Some(row) = reads.query_opt::<Registered>(ONE, bind![resource]).await? else {
        return Ok(Vec::new());
    };
    // A party-backed kind is addressed by its party; everything else by itself.
    let object = match reads
        .query_opt::<rn_kernel::store::Count>(PARTY_OF, bind![resource])
        .await?
    {
        Some(party) => party.0,
        None => resource.get(),
    };
    let mut value = row.head(key);
    let fields = value.as_object_mut().expect("a resource is an object");
    fields.insert("object_id".to_owned(), json!(object));
    fields.insert(
        "relations".to_owned(),
        as_object(reads, &row.kind, object, key).await?,
    );
    Ok(vec![Row {
        key: row.id.public(key).as_str().to_owned(),
        value,
    }])
}

struct Granted {
    relation: String,
    subject_kind: String,
    subject_id: i64,
    at: Timestamp,
}

impl FromRow for Granted {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            relation: row.text("relation")?,
            subject_kind: row.text("subject_kind")?,
            subject_id: row.int("subject_id")?,
            at: row.int("at")?,
        })
    }
}

async fn as_object(reads: &impl Reads, kind: &str, object: i64, key: &IdKey) -> Outcome<Value> {
    Ok(Value::Array(
        reads
            .query::<Granted>(AS_OBJECT, bind![kind, object])
            .await?
            .into_iter()
            .map(|row| {
                json!({
                    "relation": row.relation,
                    "subject_kind": row.subject_kind,
                    "subject": subject_id(&row.subject_kind, row.subject_id, key),
                    "at": row.at,
                })
            })
            .collect(),
    ))
}

/// A subject's public id, under the tag its kind carries.
///
/// `public` and `authenticated` are subjects with no row to point at — they
/// carry `subject_id = 0` precisely so the UNIQUE index keeps deduplicating
/// them — so they are named by their kind and nothing else. A `link` is the
/// third: a link row has no public id at all, because its token is the only
/// name it ever had and that token exists in the clear exactly once.
fn subject_id(kind: &str, id: i64, key: &IdKey) -> Value {
    match kind {
        "identity" => json!(Id::<rn_kernel::ids::Identity>::new(id).public(key)),
        "service" => json!(Id::<rn_kernel::ids::Service>::new(id).public(key)),
        "link" | "public" | "authenticated" => Value::Null,
        _ => object_id(kind, id, key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> IdKey {
        IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key")
    }

    #[test]
    fn the_subjects_with_no_row_of_their_own_carry_no_id() {
        let key = key();
        for kind in ["public", "authenticated", "link"] {
            assert_eq!(subject_id(kind, 0, &key), Value::Null, "{kind}");
        }
    }

    #[test]
    fn every_addressable_subject_kind_gets_its_own_tag() {
        let key = key();
        let ids: Vec<String> = ["person", "organization", "group", "identity", "service"]
            .iter()
            .filter_map(|kind| {
                subject_id(kind, 5, &key)
                    .as_str()
                    .map(std::borrow::ToOwned::to_owned)
            })
            .collect();
        assert_eq!(ids.len(), 5);
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "two subject kinds share an id: {ids:?}");
    }
}
