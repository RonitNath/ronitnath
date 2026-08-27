//! Public ids: the only identifier that ever leaves the process.
//!
//! A public id is a type prefix plus the base64url encoding of an AES-128
//! block over `table_tag ‖ rowid` (kernel report §Schema). The internal
//! integer `id` is never serialised, and the ciphertext is not a column: it is
//! derived on the way out and decrypted on the way in, so a forged or
//! cross-type id fails to decrypt rather than addressing the wrong row.
//!
//! The key lives in the server's config, so this crate cannot decrypt anything
//! — and does not need to. It validates the *shape*: a known prefix and a
//! 22-character base64url body. That is enough for a bundle to keep ids apart
//! and for the server to reject junk before it reaches the kernel.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Length of the base64url body: 16 ciphertext bytes, unpadded.
const BODY_LEN: usize = 22;

/// What a public id points at. The prefix is part of the id, so an id is
/// self-describing and a document id can never be read as a person id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdKind {
    /// `p_` — a person: the human, resolved from one or more identities.
    Person,
    /// `i_` — an identity: one source-scoped registration.
    Identity,
    /// `o_` — an organization: a tenant and ownership boundary.
    Organization,
    /// `g_` — a group: a set of parties, granted but never owning.
    Group,
    /// `v_` — a service principal.
    Service,
    /// `r_` — a row in the resource registry (a document, a photo, …).
    Resource,
    /// `s_` — a session.
    Session,
    /// `f_` — a factor proven for an identity.
    Factor,
    /// `m_` — a match candidate awaiting proof.
    MatchCandidate,
    /// `l_` — an invitation link.
    ///
    /// The id names the row, never the secret: a link is *claimed* by its
    /// bearer token and nothing else, and this is what a page that lists an
    /// invitation addresses when it wants to revoke one.
    Link,
}

impl IdKind {
    /// The two-character prefix, trailing underscore included.
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Person => "p_",
            Self::Identity => "i_",
            Self::Organization => "o_",
            Self::Group => "g_",
            Self::Service => "v_",
            Self::Resource => "r_",
            Self::Session => "s_",
            Self::Factor => "f_",
            Self::MatchCandidate => "m_",
            Self::Link => "l_",
        }
    }

    /// Every kind, so a caller can exhaustively map prefixes without matching.
    pub const ALL: [Self; 10] = [
        Self::Person,
        Self::Identity,
        Self::Organization,
        Self::Group,
        Self::Service,
        Self::Resource,
        Self::Session,
        Self::Factor,
        Self::MatchCandidate,
        Self::Link,
    ];
}

/// Why a string is not a public id. Deliberately coarse: this is developer
/// feedback, never a response body — a bad id declines like anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PublicIdError {
    /// The string does not start with a prefix this vocabulary knows.
    #[error("unknown public id prefix")]
    UnknownPrefix,
    /// The body is not exactly 22 base64url characters.
    #[error("malformed public id body")]
    MalformedBody,
}

/// A validated public id, kept as the string that was on the wire.
///
/// Serialises as that bare string, so `{"person": "p_AAAA…"}` is the wire
/// form; deserialising re-runs the shape check, which means a struct that
/// holds a `PublicId` cannot be built out of junk JSON.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PublicId(String);

impl PublicId {
    /// The kind the prefix names.
    pub fn kind(&self) -> IdKind {
        IdKind::ALL
            .into_iter()
            .find(|k| self.0.starts_with(k.prefix()))
            .expect("a PublicId is only constructed through a prefix check")
    }

    /// The id as it appears on the wire.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for PublicId {
    type Err = PublicIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let kind = IdKind::ALL
            .into_iter()
            .find(|k| s.starts_with(k.prefix()))
            .ok_or(PublicIdError::UnknownPrefix)?;
        let body = &s[kind.prefix().len()..];
        let base64url = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        if body.len() != BODY_LEN || !body.chars().all(base64url) {
            return Err(PublicIdError::MalformedBody);
        }
        Ok(Self(s.to_owned()))
    }
}

impl fmt::Display for PublicId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PublicId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: IdKind) -> String {
        format!("{}{}", kind.prefix(), "A".repeat(BODY_LEN))
    }

    #[test]
    fn every_kind_round_trips_through_its_prefix() {
        for kind in IdKind::ALL {
            let id: PublicId = sample(kind).parse().expect("well-formed");
            assert_eq!(id.kind(), kind);
            let json = serde_json::to_string(&id).expect("serialises");
            assert_eq!(json, format!("\"{id}\""));
            let back: PublicId = serde_json::from_str(&json).expect("deserialises");
            assert_eq!(back, id);
        }
    }

    #[test]
    fn prefixes_are_distinct() {
        let mut seen: Vec<&str> = IdKind::ALL.iter().map(|k| k.prefix()).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), count);
    }

    #[test]
    fn junk_is_refused_rather_than_carried() {
        let body = "A".repeat(BODY_LEN);
        assert_eq!(
            format!("z_{body}").parse::<PublicId>(),
            Err(PublicIdError::UnknownPrefix)
        );
        assert_eq!(
            "p_short".parse::<PublicId>(),
            Err(PublicIdError::MalformedBody)
        );
        assert_eq!(
            format!("p_{}", "+".repeat(BODY_LEN)).parse::<PublicId>(),
            Err(PublicIdError::MalformedBody)
        );
        assert!(serde_json::from_str::<PublicId>("\"nonsense\"").is_err());
    }
}
