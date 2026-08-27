//! Public ids: the only identifier that ever leaves the process.
//!
//! The internal identifier is the rowid — a small integer, dense, cheap to
//! index and join on, and hopeless as a public name: it leaks how many rows
//! there are and lets a caller walk them. So a row's public name is derived
//! rather than stored:
//!
//! ```text
//!   plaintext   kind_tag: u64 (big endian)  ‖  rowid: i64 (big endian)   = 16 bytes
//!   ciphertext  AES-128 over that single block                           = 16 bytes
//!   wire        "<prefix>" ‖ base64url(ciphertext), unpadded             = 2 + 22 chars
//! ```
//!
//! One AES block is exactly the plaintext width, so there is no mode, no IV
//! and no padding to get wrong — and because the mapping is a bijection, no
//! column and no lookup table. `p_…` and `o_…` are both party rowids, but they
//! carry different tags, so an organization id offered where a person id
//! belongs fails to decrypt rather than addressing the wrong party. The
//! *prefix* is checked first and the *tag* after decryption, so a forged id, an
//! id from another deployment's key, and an id of the wrong kind are all
//! refused before the database is touched.
//!
//! [`Id<T>`] is the internal form. It deliberately implements no `Serialize`:
//! the only way an id reaches a wire type is [`Id::public`], which needs the
//! key, and the only way one comes back is [`decode`], which validates it.

use std::fmt;
use std::marker::PhantomData;

use aes::cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes::{Aes128Dec, Aes128Enc};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use rn_api::ids::PublicId;

mod tables;
#[cfg(test)]
mod tests;

pub use tables::{
    Factor, Group, Identity, Link, MatchCandidate, Organization, Person, Public, Resource, Service,
    Session, Table,
};

/// Width of the AES block the id occupies, and of the ciphertext.
const BLOCK: usize = 16;

/// Why a string is not an id of the kind that was asked for.
///
/// Callers do not show these to anyone: a bad id declines like everything else
/// (`error::Decline`). They exist so a test can say *which* rejection it
/// proved, and so a log line can tell a typo from an attack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// The key was not 32 hexadecimal characters.
    #[error("an id key is 32 hex characters")]
    BadKey,
    /// The prefix names a different kind than the caller asked to decode.
    #[error("public id prefix names the wrong kind")]
    PrefixMismatch,
    /// The body was not 22 base64url characters over 16 bytes.
    #[error("public id body is not one base64url block")]
    MalformedBody,
    /// The plaintext's tag is not the kind's. Either the id was forged, or it
    /// was minted under a different key, or it names another kind entirely —
    /// which of the three is not knowable and not useful.
    #[error("public id does not decrypt to this kind")]
    TagMismatch,
    /// The plaintext decrypted to a rowid no table can hold.
    #[error("public id decrypts to an impossible row")]
    ImpossibleRow,
}

/// The AES-128 key public ids are derived under.
///
/// One key per deployment, from config, and never logged: it is not a secret
/// in the sense a password is — knowing it reveals only rowids — but it is
/// what stops a caller minting ids for rows it has never seen, so it is
/// handled like one. `Debug` prints nothing.
#[derive(Clone)]
pub struct IdKey {
    encrypt: Aes128Enc,
    decrypt: Aes128Dec,
}

impl IdKey {
    /// Parse the 32 hex characters config carries.
    pub fn from_hex(hex: &str) -> Result<Self, IdError> {
        let bytes = hex.as_bytes();
        if bytes.len() != BLOCK * 2 {
            return Err(IdError::BadKey);
        }
        let mut key = [0u8; BLOCK];
        for (i, byte) in key.iter_mut().enumerate() {
            let hi = nibble(bytes[i * 2]).ok_or(IdError::BadKey)?;
            let lo = nibble(bytes[i * 2 + 1]).ok_or(IdError::BadKey)?;
            *byte = (hi << 4) | lo;
        }
        Ok(Self::from_bytes(key))
    }

    /// Build a key from raw bytes. Config goes through [`Self::from_hex`];
    /// this is for a caller that already holds 16 bytes.
    pub fn from_bytes(key: [u8; BLOCK]) -> Self {
        let key = Array(key);
        Self {
            encrypt: Aes128Enc::new(&key),
            decrypt: Aes128Dec::new(&key),
        }
    }
}

impl fmt::Debug for IdKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IdKey(..)")
    }
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// A row identifier, inside the process.
///
/// The type parameter is what stops a `Id<Session>` reaching a query that
/// wanted an `Id<Identity>`: both are `i64` at runtime and neither is
/// assignable to the other.
pub struct Id<T: Table>(i64, PhantomData<fn() -> T>);

impl<T: Table> Id<T> {
    /// Wrap a rowid the database just produced or returned.
    pub const fn new(raw: i64) -> Self {
        Self(raw, PhantomData)
    }

    /// The rowid, for binding into a statement.
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl<T: Public> Id<T> {
    /// Derive the public id this row is known by outside the process.
    pub fn public(self, key: &IdKey) -> PublicId {
        let mut block = Array([0u8; BLOCK]);
        block[..8].copy_from_slice(&T::TAG.to_be_bytes());
        block[8..].copy_from_slice(&self.0.to_be_bytes());
        key.encrypt.encrypt_block(&mut block);

        let id = format!("{}{}", T::KIND.prefix(), B64.encode(block.as_slice()));
        id.parse()
            .expect("a derived id is a known prefix over 22 base64url characters")
    }
}

/// Recover the row a public id names, or refuse it.
///
/// Every refusal happens here, before any query: the prefix must be the kind
/// the caller asked for, the body must be one base64url block, and the
/// plaintext's tag must match. A caller therefore never has to ask the
/// database whether an id is real.
pub fn decode<T: Public>(key: &IdKey, id: &PublicId) -> Result<Id<T>, IdError> {
    if id.kind() != T::KIND {
        return Err(IdError::PrefixMismatch);
    }
    let body = &id.as_str()[T::KIND.prefix().len()..];
    let mut block = Array([0u8; BLOCK]);
    let written = B64
        .decode_slice(body.as_bytes(), block.as_mut_slice())
        .map_err(|_| IdError::MalformedBody)?;
    if written != BLOCK {
        return Err(IdError::MalformedBody);
    }

    key.decrypt.decrypt_block(&mut block);
    let (tag, row) = block.split_at(8);
    if u64::from_be_bytes(tag.try_into().expect("eight bytes")) != T::TAG {
        return Err(IdError::TagMismatch);
    }
    let row = i64::from_be_bytes(row.try_into().expect("eight bytes"));
    if row <= 0 {
        return Err(IdError::ImpossibleRow);
    }
    Ok(Id::new(row))
}

/// Parse a wire string and recover the row in one step, for a caller holding
/// text rather than a validated [`PublicId`].
pub fn parse<T: Public>(key: &IdKey, raw: &str) -> Result<Id<T>, IdError> {
    let id: PublicId = raw.parse().map_err(|_| IdError::MalformedBody)?;
    decode(key, &id)
}

// `Id` derives nothing: a derive would bound `T`, and the marker types are
// uninhabited-in-spirit tags that need none of these traits themselves.
impl<T: Table> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Table> Copy for Id<T> {}

impl<T: Table> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Table> Eq for Id<T> {}

impl<T: Table> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Table> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<T: Table> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// Prints the rowid and the table, never a public id: this is a log line for
/// us, and a public id in a log is a public id in an incident report.
impl<T: Table> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", T::NAME, self.0)
    }
}
