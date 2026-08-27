//! Naming a `party` row on the wire, when the row is all you have.
//!
//! Four kinds share the `party` table and they must not share a tag: one tag
//! would make `o_…` and `p_…` decrypt to each other, which is the exact
//! confusion the encryption exists to prevent. So a public id cannot be
//! derived from a party's rowid alone — the kind has to come from somewhere.
//!
//! Where it comes from depends on the caller. `whoami` has already loaded the
//! row, so it passes the kind it read. A command's event has not: several
//! events type a party field as `Id<Person>` because that is the *table*, not
//! the kind — `Transfer`'s new owner may be an organization, a group's owner
//! usually is — and rendering those under the person tag would hand a caller
//! back an id that is not the one it sent. So this module also asks the row.

use rn_api::PublicId;
use rn_kernel::bind;
use rn_kernel::domain::{PartyKind, Vocabulary as _};
use rn_kernel::ids::{Group, Id, IdKey, Organization, Person, Service, Table};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};

/// The kind of one party row. The narrowest read there is: an id is all this
/// answer needs.
const KIND: &str = "SELECT kind FROM party WHERE id = $1";

struct Kind(PartyKind);

impl FromRow for Kind {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let raw = row.text("kind")?;
        PartyKind::parse(&raw).map(Self).ok_or(RowError::Column {
            column: "kind".to_owned(),
            wanted: "PartyKind",
        })
    }
}

/// A party's public id, derived under the tag its kind carries.
#[must_use]
pub fn public_of(kind: PartyKind, raw: i64, key: &IdKey) -> PublicId {
    match kind {
        PartyKind::Person => Id::<Person>::new(raw).public(key),
        PartyKind::Organization => Id::<Organization>::new(raw).public(key),
        PartyKind::Group => Id::<Group>::new(raw).public(key),
        PartyKind::Service => Id::<Service>::new(raw).public(key),
    }
}

/// A party's public id, with the kind read from the row.
///
/// `None` when the row is gone or carries a kind this build cannot name — an
/// id derived under a guessed tag would be worse than no id, because it would
/// decode successfully as the wrong thing.
pub async fn public_id(reads: &impl Reads, raw: i64, key: &IdKey) -> Option<PublicId> {
    reads
        .query_opt::<Kind>(KIND, bind![raw])
        .await
        .ok()
        .flatten()
        .map(|kind| public_of(kind.0, raw, key))
}

/// Assert at compile time that every party kind has its own tag.
const _: () = {
    assert!(<Person as Table>::TAG != <Organization as Table>::TAG);
    assert!(<Group as Table>::TAG != <Service as Table>::TAG);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_row_gets_a_different_id_under_every_party_kind() {
        let key = IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key");
        let kinds = [
            PartyKind::Person,
            PartyKind::Organization,
            PartyKind::Group,
            PartyKind::Service,
        ];
        let ids: Vec<String> = kinds
            .iter()
            .map(|kind| public_of(*kind, 7, &key).as_str().to_owned())
            .collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            ids.len(),
            "two party kinds share an id: {ids:?}"
        );
        assert!(ids[0].starts_with("p_"));
        assert!(ids[1].starts_with("o_"));
    }
}
