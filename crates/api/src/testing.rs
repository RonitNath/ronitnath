//! Helpers the round-trip tests share. Compiled only under `cfg(test)`.

use std::fmt::Debug;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::ids::PublicId;

/// A well-formed public id of the given prefix, for fixtures.
pub(crate) fn id(prefix: &str) -> PublicId {
    format!("{prefix}{}", "A".repeat(22))
        .parse()
        .expect("fixture prefixes are part of the vocabulary")
}

/// Assert a value survives a trip through JSON unchanged. Every wire type is
/// held to this: a shape that cannot come back is not a wire shape.
pub(crate) fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
    let json = serde_json::to_string(value).expect("serialises");
    let back: T = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(&back, value, "round trip changed the value: {json}");
}
