//! Internal vs public identifiers.
//!
//! `InternalId` is the integer primary key used for every join inside the
//! server. It deliberately does **not** implement `Serialize` / `Deserialize`
//! so it cannot cross the client boundary by accident.
//!
//! `PublicId` is a UUID v4 minted at insert time. It is the only identifier
//! that may leave the process. Resolve it through [`PublicIdIndex`].

use std::collections::HashMap;
use std::fmt;

use uuid::Uuid;

/// Server-local row id. Never send this to a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InternalId(i64);

impl InternalId {
    #[must_use]
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl fmt::Display for InternalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display is for server logs only — still never serialize to clients.
        write!(f, "{}", self.0)
    }
}

/// Opaque external identifier (UUID v4). Safe to put on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct PublicId(Uuid);

impl PublicId {
    #[must_use]
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }

    #[must_use]
    pub fn to_string_lossy(&self) -> String {
        self.0.to_string()
    }
}

impl fmt::Display for PublicId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for PublicId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Which entity family a public id belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PublicIdKind {
    Identity,
    Account,
}

/// In-process `public_id → id` resolution. Populated on insert / read-through
/// from SQLite; never exposed to clients.
#[derive(Debug, Default)]
pub struct PublicIdIndex {
    identities: HashMap<PublicId, InternalId>,
    accounts: HashMap<PublicId, InternalId>,
}

impl PublicIdIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, kind: PublicIdKind, public_id: PublicId, id: InternalId) {
        match kind {
            PublicIdKind::Identity => {
                self.identities.insert(public_id, id);
            }
            PublicIdKind::Account => {
                self.accounts.insert(public_id, id);
            }
        }
    }

    #[must_use]
    pub fn resolve(&self, kind: PublicIdKind, public_id: PublicId) -> Option<InternalId> {
        match kind {
            PublicIdKind::Identity => self.identities.get(&public_id).copied(),
            PublicIdKind::Account => self.accounts.get(&public_id).copied(),
        }
    }

    #[must_use]
    pub fn resolve_identity(&self, public_id: PublicId) -> Option<InternalId> {
        self.resolve(PublicIdKind::Identity, public_id)
    }

    #[must_use]
    pub fn resolve_account(&self, public_id: PublicId) -> Option<InternalId> {
        self.resolve(PublicIdKind::Account, public_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_id_round_trips_as_string() {
        let id = PublicId::new_v4();
        let parsed: PublicId = id.to_string().parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn index_resolves_by_kind() {
        let mut index = PublicIdIndex::new();
        let pub_id = PublicId::new_v4();
        let internal = InternalId::new(7);
        index.insert(PublicIdKind::Identity, pub_id, internal);
        assert_eq!(index.resolve_identity(pub_id), Some(internal));
        assert_eq!(index.resolve_account(pub_id), None);
    }
}
