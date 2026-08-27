//! Id tests. The keys here are obviously fake constants: they name themselves
//! as test keys so nobody mistakes one for a deployment's.

use proptest::prelude::*;
use rn_api::ids::IdKind;

use super::*;

/// A key that says what it is. Never a deployment's.
const TEST_KEY: &str = "00112233445566778899aabbccddeeff";
/// A second one, for "minted under a different key" cases.
const OTHER_KEY: &str = "ffeeddccbbaa99887766554433221100";

fn key() -> IdKey {
    IdKey::from_hex(TEST_KEY).expect("test key is well formed")
}

fn other() -> IdKey {
    IdKey::from_hex(OTHER_KEY).expect("test key is well formed")
}

#[test]
fn a_key_is_thirty_two_hex_characters_and_prints_nothing() {
    assert!(matches!(IdKey::from_hex("abc"), Err(IdError::BadKey)));
    assert!(matches!(
        IdKey::from_hex("zz112233445566778899aabbccddeeff"),
        Err(IdError::BadKey)
    ));
    assert_eq!(format!("{:?}", key()), "IdKey(..)");
    assert!(IdKey::from_hex(&TEST_KEY.to_uppercase()).is_ok());
}

#[test]
fn every_kind_round_trips_and_wears_its_prefix() {
    let key = key();
    macro_rules! check {
        ($($t:ty),+) => {$({
            let id = Id::<$t>::new(4_242);
            let public = id.public(&key);
            assert_eq!(public.kind(), <$t as Public>::KIND);
            assert!(public.as_str().starts_with(<$t as Public>::KIND.prefix()));
            assert_eq!(public.as_str().len(), 24, "prefix plus one base64url block");
            assert_eq!(decode::<$t>(&key, &public).expect("round trips"), id);
        })+};
    }
    check!(
        Person,
        Organization,
        Group,
        Service,
        Identity,
        Factor,
        Session,
        Resource,
        MatchCandidate
    );
}

#[test]
fn the_same_row_under_two_kinds_is_two_different_ids() {
    let key = key();
    let person = Id::<Person>::new(7).public(&key);
    let organization = Id::<Organization>::new(7).public(&key);
    assert_ne!(person.as_str()[2..], organization.as_str()[2..]);
}

#[test]
fn an_id_of_another_kind_is_refused_on_its_prefix() {
    let key = key();
    let organization = Id::<Organization>::new(7).public(&key);
    assert_eq!(
        decode::<Person>(&key, &organization),
        Err(IdError::PrefixMismatch)
    );
}

#[test]
fn an_id_wearing_the_right_prefix_over_the_wrong_tag_is_refused() {
    // The attack the prefix check alone would miss: take an organization's
    // ciphertext, relabel it `p_`, and offer it where a person belongs.
    let key = key();
    let organization = Id::<Organization>::new(7).public(&key);
    let relabelled: rn_api::ids::PublicId = format!("p_{}", &organization.as_str()[2..])
        .parse()
        .expect("still a well-formed public id");
    assert_eq!(relabelled.kind(), IdKind::Person);
    assert_eq!(
        decode::<Person>(&key, &relabelled),
        Err(IdError::TagMismatch)
    );
}

#[test]
fn an_id_minted_under_another_key_is_refused() {
    let mine = Id::<Identity>::new(9).public(&key());
    assert_eq!(
        decode::<Identity>(&other(), &mine),
        Err(IdError::TagMismatch)
    );
}

#[test]
fn a_body_that_is_not_one_block_is_refused() {
    // 22 base64url characters that decode to 16 bytes is the only accepted
    // body, and `PublicId` already enforces the length; what reaches `decode`
    // with a wrong width is a body whose final character carries stray bits.
    let key = key();
    let short: rn_api::ids::PublicId = format!("i_{}", "A".repeat(22))
        .parse()
        .expect("well-formed shape");
    // Sixteen zero bytes decrypt to something whose tag is not ours.
    assert!(matches!(
        decode::<Identity>(&key, &short),
        Err(IdError::TagMismatch)
    ));
    assert!(matches!(
        parse::<Identity>(&key, "not an id"),
        Err(IdError::MalformedBody)
    ));
}

#[test]
fn debug_names_the_table_and_the_row_and_never_a_public_id() {
    assert_eq!(format!("{:?}", Id::<Session>::new(12)), "session#12");
    assert_eq!(format!("{:?}", Id::<Person>::new(12)), "party#12");
}

proptest! {
    /// Any rowid a table can hold survives the round trip exactly.
    #[test]
    fn round_trip_is_exact_for_every_rowid(row in 1i64..=i64::MAX) {
        let key = key();
        let id = Id::<Resource>::new(row);
        prop_assert_eq!(decode::<Resource>(&key, &id.public(&key)), Ok(id));
    }

    /// A forged body — 22 characters a caller made up — is refused without the
    /// database being consulted. AES makes a collision with our tag a 2^-64
    /// event; the property is that we never *accept* one by accident.
    #[test]
    fn a_forged_body_is_refused(body in "[A-Za-z0-9_-]{22}") {
        let key = key();
        let Ok(candidate) = format!("i_{body}").parse::<rn_api::ids::PublicId>() else {
            return Ok(());
        };
        match decode::<Identity>(&key, &candidate) {
            Err(_) => {}
            // The one way to be right is to have guessed a real ciphertext,
            // which means round-tripping back to the same string.
            Ok(id) => prop_assert_eq!(id.public(&key), candidate),
        }
    }

    /// An id of one kind never decodes as another, whatever the row.
    #[test]
    fn tags_do_not_cross(row in 1i64..=1_000_000i64) {
        let key = key();
        let session = Id::<Session>::new(row).public(&key);
        let relabelled: rn_api::ids::PublicId =
            format!("f_{}", &session.as_str()[2..]).parse().expect("shape holds");
        prop_assert_eq!(decode::<Factor>(&key, &relabelled), Err(IdError::TagMismatch));
    }
}
