//! One box, three answers.
//!
//! The parties list is a bounded page of the newest rows, which is the wrong
//! shape for "find this person" the moment a deployment has more parties than
//! a page. This is the right shape, and what makes it right is what it refuses
//! to do: there is no prefix scan over `display_name`, because `display_name`
//! has no index and a search box that walks every party on the deployment is a
//! search box that stops working exactly when somebody needs it.
//!
//! So three exact seeks, each on an index that already exists:
//!
//! * a **handle** — `party_handle_unique_idx`;
//! * an **email factor** — `factor_email_unique_idx`, the partial unique index
//!   over `value WHERE kind = 'email'`;
//! * a **public id** — the id decrypt, which is not a statement at all: a
//!   forged or malformed id fails the tag check in process and names nothing.
//!
//! Each seek is attempted only when the query string *could* be that kind of
//! thing, so a forty-character piece of junk runs no statement: it is not a
//! handle (too long, wrong alphabet), it is not an address (no `@`), and it is
//! not an id (it does not decrypt). The answer is an empty list.
//!
//! And a miss is empty, never a decline. An operator searching for somebody
//! who is not there is not being probed — they already hold the whole
//! deployment — and a decline would only teach them to search twice.

use rn_api::PublicId;
use rn_kernel::bind;
use rn_kernel::domain::{PartyKind, Vocabulary as _};
use rn_kernel::ids::{self, Group, IdKey, Identity, Organization, Person, Service};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::{Params, Row};
use crate::api::party::public_of;

/// A handle is lowercase `[a-z0-9-]`, 3–32 characters. Anything else is not a
/// handle and there is no reason to ask the database about it.
const HANDLE_MAX: usize = 32;
const HANDLE_MIN: usize = 3;

/// The longest thing worth putting in front of any of the three seeks. An
/// address is bounded by the factor column's own use; nothing a human types
/// into this box is longer than this, and a query string that is becomes an
/// empty answer without touching the database.
const Q_MAX: usize = 320;

/// By handle.
pub(super) const BY_HANDLE: &str =
    "SELECT id, kind, display_name, status, handle, created_at FROM party WHERE handle = $1";

/// By verified-or-not email factor, through the registration to the person.
///
/// `f.kind = 'email'` is not redundant with `f.value = $1`: it is what lets
/// SQLite use the *partial* unique index, whose own condition is that clause.
pub(super) const BY_EMAIL: &str = "SELECT p.id, p.kind, p.display_name, p.status, p.handle, \
       p.created_at \
     FROM factor f JOIN identity i ON i.id = f.identity_id \
     JOIN party p ON p.id = i.person_id \
     WHERE f.kind = 'email' AND f.value = $1";

/// By party rowid, once an id has decrypted to one.
pub(super) const BY_ID: &str =
    "SELECT id, kind, display_name, status, handle, created_at FROM party WHERE id = $1";

/// The person a registration resolved to, for an identity id typed into the
/// box. `identity` is addressed by its own primary key.
pub(super) const PERSON_OF_IDENTITY: &str = "SELECT p.id, p.kind, p.display_name, p.status, \
       p.handle, p.created_at \
     FROM identity i JOIN party p ON p.id = i.person_id WHERE i.id = $1";

struct Found {
    id: i64,
    kind: PartyKind,
    display_name: String,
    status: String,
    handle: Option<String>,
    created_at: Timestamp,
}

impl FromRow for Found {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let raw = row.text("kind")?;
        Ok(Self {
            id: row.int("id")?,
            kind: PartyKind::parse(&raw).ok_or(RowError::Column {
                column: "kind".to_owned(),
                wanted: "PartyKind",
            })?,
            display_name: row.text("display_name")?,
            status: row.text("status")?,
            handle: row.text_opt("handle")?,
            created_at: row.int("created_at")?,
        })
    }
}

/// Whether this could be a handle at all.
#[must_use]
pub fn is_handle(q: &str) -> bool {
    (HANDLE_MIN..=HANDLE_MAX).contains(&q.chars().count())
        && q.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether this could be an address at all.
///
/// One `@`, something on each side. Deliberately not a validator: the column
/// holds whatever registration accepted, and this only decides whether asking
/// is worth a statement.
#[must_use]
pub fn is_address(q: &str) -> bool {
    match q.split_once('@') {
        Some((local, domain)) => !local.is_empty() && domain.contains('.'),
        None => false,
    }
}

/// The party rowid a public id names, whichever kind it was minted under.
///
/// The four party tags address `party` directly. An identity id is admitted
/// too and answers with the person it resolved to, because an operator with an
/// identity id in front of them is looking for a human and not for a row.
fn party_of(key: &IdKey, q: &str) -> Option<(&'static str, i64)> {
    let id: PublicId = q.parse().ok()?;
    let party = [
        ids::decode::<Person>(key, &id).ok().map(|row| row.get()),
        ids::decode::<Organization>(key, &id)
            .ok()
            .map(|row| row.get()),
        ids::decode::<Group>(key, &id).ok().map(|row| row.get()),
        ids::decode::<Service>(key, &id).ok().map(|row| row.get()),
    ]
    .into_iter()
    .flatten()
    .next();
    match party {
        Some(row) => Some((BY_ID, row)),
        None => ids::decode::<Identity>(key, &id)
            .ok()
            .map(|identity| (PERSON_OF_IDENTITY, identity.get())),
    }
}

/// `/api/q/platform-find?q=`.
pub(super) async fn find(reads: &impl Reads, params: &Params) -> Outcome<Vec<Row>> {
    let key: &IdKey = reads.ids();
    let Some(q) = params.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) else {
        return Ok(Vec::new());
    };
    if q.chars().count() > Q_MAX {
        return Ok(Vec::new());
    }

    let mut found: Vec<(&'static str, Found)> = Vec::new();
    if is_handle(q) {
        found.extend(
            reads
                .query::<Found>(BY_HANDLE, bind![q])
                .await?
                .into_iter()
                .map(|row| ("handle", row)),
        );
    }
    if is_address(q) {
        found.extend(
            reads
                .query::<Found>(BY_EMAIL, bind![q])
                .await?
                .into_iter()
                .map(|row| ("email", row)),
        );
    }
    if let Some((sql, id)) = party_of(key, q) {
        found.extend(
            reads
                .query::<Found>(sql, bind![id])
                .await?
                .into_iter()
                .map(|row| ("id", row)),
        );
    }

    // Two seeks can name the same party — somebody who typed their own handle
    // into a deployment that also derived it from their address. One row, and
    // it keeps the first `via`, which is the seek that answered first.
    let mut rows: Vec<Row> = Vec::new();
    for (via, row) in found {
        let public = public_of(row.kind, row.id, key);
        let key_str = public.as_str().to_owned();
        if rows.iter().any(|existing| existing.key == key_str) {
            continue;
        }
        rows.push(Row {
            key: key_str,
            value: json!({
                "public_id": public,
                "kind": row.kind.as_str(),
                "display": row.display_name,
                "handle": row.handle,
                "status": row.status,
                "created_at": row.created_at,
                // Which of the three found it. The screen states it rather
                // than implying the box is a full-text search.
                "via": via,
            }),
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rn_kernel::ids::Id;

    fn key() -> IdKey {
        IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key")
    }

    #[test]
    fn only_a_handle_shaped_string_asks_the_handle_index() {
        assert!(is_handle("bea"));
        assert!(is_handle("ronit-nath"));
        assert!(is_handle("a1b2c3"));
        // Too short, too long, wrong alphabet, and an address.
        assert!(!is_handle("ab"));
        assert!(!is_handle(&"a".repeat(33)));
        assert!(!is_handle("Bea"));
        assert!(!is_handle("bea okafor"));
        assert!(!is_handle("bea@example.test"));
    }

    #[test]
    fn only_an_address_shaped_string_asks_the_factor_index() {
        assert!(is_address("ronit@isoastra.com"));
        assert!(!is_address("ronit"));
        assert!(!is_address("@isoastra.com"));
        assert!(!is_address("ronit@localhost"));
    }

    #[test]
    fn forty_characters_of_junk_matches_none_of_the_three_shapes() {
        let junk = "9x_".repeat(14);
        assert_eq!(junk.chars().count(), 42);
        assert!(!is_handle(&junk));
        assert!(!is_address(&junk));
        assert!(party_of(&key(), &junk).is_none());
    }

    #[test]
    fn a_public_id_decrypts_to_the_row_it_names_and_no_other() {
        let key = key();
        let person = Id::<Person>::new(7).public(&key);
        assert_eq!(party_of(&key, person.as_str()), Some((BY_ID, 7)));
        let identity = Id::<Identity>::new(11).public(&key);
        assert_eq!(
            party_of(&key, identity.as_str()),
            Some((PERSON_OF_IDENTITY, 11)),
            "an identity id answers with the human it resolved to"
        );
        // A well-formed id under a key this deployment does not hold names
        // nothing, and says so by finding nothing.
        let other = IdKey::from_hex("0f0e0d0c0b0a09080706050403020100").expect("a second key");
        let forged = Id::<Person>::new(7).public(&other);
        assert!(party_of(&key, forged.as_str()).is_none());
    }
}
