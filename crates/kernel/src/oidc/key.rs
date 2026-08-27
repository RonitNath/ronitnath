//! The signing keys, and the two things a deployment does with them: sign an
//! id token, and publish the public half.
//!
//! RS256 and one algorithm, because every relying party implements it and a
//! second one is a second thing to get wrong. Keys are 2048-bit RSA, minted by
//! the `rotate-signing-key` command, and they live in three statuses:
//!
//! * `active` — what new tokens are signed under. Exactly one.
//! * `retiring` — no longer signs, still verifies, still published in the
//!   JWKS. This is the overlap that makes rotation a non-event for an RP that
//!   caches the document.
//! * `retired` — gone from the JWKS. A token signed under it no longer
//!   verifies anywhere, which is the point of retiring it.
//!
//! The private half never sits in the database in the clear. It is sealed with
//! AES-256-GCM under [`SealKey`], which comes from `RN_SITE__OIDC_KEY` (or the
//! file `RN_SITE__OIDC_KEY_FILE` names) and is *not* the id key: the id key
//! names rows, and this one forges identity. One variable that did both would
//! be one leak that did both.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use rsa::pkcs8::{DecodePrivateKey as _, EncodePrivateKey as _};
use rsa::traits::PublicKeyParts as _;
use rsa::{RsaPrivateKey, RsaPublicKey};

use crate::Timestamp;
use crate::error::{KernelError, Outcome};
use crate::ids::{Id, OidcKey};
use crate::store::{Cursor, FromRow, Reads, RowError};
use crate::{bind, error::decline};

/// Bits in a signing key. 2048 is what every RP verifies and what RFC 7518
/// §3.3 makes the floor for RS256.
pub const KEY_BITS: usize = 2048;

/// Bytes in the sealing key, and in the GCM nonce that prefixes a ciphertext.
const SEAL_KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;

/// The key the private halves are sealed under, at rest.
///
/// `Debug` prints nothing, for the reason [`crate::domain::Token`]'s does: the
/// way a secret reaches a log is that some struct three layers away derived
/// it.
#[derive(Clone)]
pub struct SealKey([u8; SEAL_KEY_BYTES]);

impl std::fmt::Debug for SealKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SealKey(..)")
    }
}

impl SealKey {
    /// Parse the 64 hex characters config carries.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let bytes = hex.as_bytes();
        if bytes.len() != SEAL_KEY_BYTES * 2 {
            return None;
        }
        let mut key = [0u8; SEAL_KEY_BYTES];
        for (i, byte) in key.iter_mut().enumerate() {
            let hi = nibble(bytes[i * 2])?;
            let lo = nibble(bytes[i * 2 + 1])?;
            *byte = (hi << 4) | lo;
        }
        Some(Self(key))
    }

    /// A fresh key, for a dev process and for a test. Never for a deployment:
    /// a key minted per boot is a JWKS that changes per boot.
    ///
    /// # Panics
    ///
    /// If the operating system cannot produce randomness.
    #[must_use]
    pub fn mint() -> Self {
        let mut key = [0u8; SEAL_KEY_BYTES];
        getrandom::fill(&mut key).expect("the operating system must provide randomness");
        Self(key)
    }

    /// The key as config writes it, for a dev process to print once.
    #[must_use]
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn cipher(&self) -> aes_gcm::Aes256Gcm {
        use aes_gcm::KeyInit as _;
        aes_gcm::Aes256Gcm::new((&self.0).into())
    }

    /// Seal a private key: a fresh nonce, then the ciphertext.
    fn seal(&self, plaintext: &[u8]) -> Outcome<Vec<u8>> {
        use aes_gcm::aead::Aead as _;
        let mut nonce = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce)
            .map_err(|_| KernelError::Invariant("no randomness for a key nonce".to_owned()))?;
        let sealed = self
            .cipher()
            .encrypt((&nonce).into(), plaintext)
            .map_err(|_| KernelError::Invariant("a signing key would not seal".to_owned()))?;
        let mut out = Vec::with_capacity(NONCE_BYTES + sealed.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// Open one. A ciphertext that will not open under this key is the wrong
    /// key, which is a deployment misconfiguration and not a caller's fault.
    fn open(&self, sealed: &[u8]) -> Outcome<Vec<u8>> {
        use aes_gcm::aead::Aead as _;
        if sealed.len() <= NONCE_BYTES {
            return Err(KernelError::Invariant(
                "a sealed key is truncated".to_owned(),
            ));
        }
        let (nonce, body) = sealed.split_at(NONCE_BYTES);
        self.cipher()
            .decrypt(nonce.into(), body)
            .map_err(|_| KernelError::Invariant("a signing key will not open".to_owned()))
    }
}

const fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// What a key row is worth once it is open: the pair, and the `kid` a JWT
/// header names it by.
pub struct SigningKey {
    /// The `kid`.
    pub kid: String,
    /// The private half.
    pub private: RsaPrivateKey,
}

/// One `oidc_key` row, as everything but the signer reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRow {
    /// The rowid.
    pub id: Id<OidcKey>,
    /// What a JWT header names it by.
    pub kid: String,
    /// `active`, `retiring` or `retired`.
    pub status: String,
    /// The JWK modulus, base64url.
    pub modulus: String,
    /// The JWK exponent, base64url.
    pub exponent: String,
    /// When it was minted.
    pub created_at: Timestamp,
}

impl FromRow for KeyRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            id: row.id("id")?,
            kid: row.text("kid")?,
            status: row.text("status")?,
            modulus: row.text("modulus")?,
            exponent: row.text("exponent")?,
            created_at: row.int("created_at")?,
        })
    }
}

/// The columns [`KeyRow`] reads.
pub const KEY_COLUMNS: &str = "id, kid, status, modulus, exponent, created_at";

/// The active key and the retiring ones, newest first — what the JWKS
/// publishes and what a verifier looks a `kid` up in.
pub const PUBLISHED_SQL: &str = "SELECT id, kid, status, modulus, exponent, created_at \
     FROM oidc_key WHERE status IN ('active', 'retiring') ORDER BY id DESC";

/// The one key new tokens are signed under.
pub const ACTIVE_SQL: &str = "SELECT id, kid, status, modulus, exponent, sealed, created_at \
     FROM oidc_key WHERE status = 'active' ORDER BY id DESC LIMIT 1";

/// One key's public half by `kid`, for verifying a token somebody presented.
pub const BY_KID_SQL: &str = "SELECT id, kid, status, modulus, exponent, created_at \
     FROM oidc_key WHERE kid = $1 AND status IN ('active', 'retiring')";

/// Mint a key row.
pub const INSERT_SQL: &str = "INSERT INTO oidc_key \
     (kid, status, modulus, exponent, sealed, created_at) \
     VALUES ($1, 'active', $2, $3, $4, $5) RETURNING id";

/// Move whatever was active out of the way. Runs before the insert above, in
/// the same batch, so there is never a moment with two active keys.
pub const RETIRE_ACTIVE_SQL: &str =
    "UPDATE oidc_key SET status = 'retiring' WHERE status = 'active'";

/// Retire a key that is still published.
pub const RETIRE_SQL: &str = "UPDATE oidc_key SET status = 'retired', retired_at = $1 \
     WHERE kid = $2 AND status = 'retiring'";

struct Sealed {
    kid: String,
    sealed: Vec<u8>,
}

impl FromRow for Sealed {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            kid: row.text("kid")?,
            sealed: row.blob("sealed")?,
        })
    }
}

/// The keys the JWKS publishes.
pub async fn published(store: &impl Reads) -> Outcome<Vec<KeyRow>> {
    Ok(store.query::<KeyRow>(PUBLISHED_SQL, bind![]).await?)
}

/// One published key by `kid`.
pub async fn by_kid(store: &impl Reads, kid: &str) -> Outcome<Option<KeyRow>> {
    Ok(store.query_opt::<KeyRow>(BY_KID_SQL, bind![kid]).await?)
}

/// The key to sign with, opened.
///
/// A deployment with no key at all declines rather than minting one on the
/// way past: a key that appears because somebody asked for a token is a key
/// nobody decided to have.
pub async fn active(store: &impl Reads, seal: &SealKey) -> Outcome<SigningKey> {
    let Some(row) = store.query_opt::<Sealed>(ACTIVE_SQL, bind![]).await? else {
        return decline();
    };
    let der = seal.open(&row.sealed)?;
    let private = RsaPrivateKey::from_pkcs8_der(&der)
        .map_err(|_| KernelError::Invariant("a sealed signing key is not a key".to_owned()))?;
    Ok(SigningKey {
        kid: row.kid,
        private,
    })
}

/// A freshly minted key, ready to be written: its `kid`, its two public JWK
/// members, and its private half sealed.
pub struct MintedKey {
    /// The `kid`.
    pub kid: String,
    /// The JWK modulus.
    pub modulus: String,
    /// The JWK exponent.
    pub exponent: String,
    /// The sealed private half.
    pub sealed: Vec<u8>,
}

/// Mint a signing key. Slow — RSA key generation is — and run once per
/// rotation, off the reactor by the command that calls it.
///
/// # Panics
///
/// Never: every failure is an [`Outcome`].
pub fn mint(seal: &SealKey) -> Outcome<MintedKey> {
    let mut rng = rand_core::OsRng;
    let private = RsaPrivateKey::new(&mut rng, KEY_BITS)
        .map_err(|err| KernelError::Invariant(format!("no signing key: {err}")))?;
    let der = private
        .to_pkcs8_der()
        .map_err(|err| KernelError::Invariant(format!("a key that will not encode: {err}")))?;
    let (modulus, exponent) = jwk_parts(&RsaPublicKey::from(&private));
    let mut kid_bytes = [0u8; 16];
    getrandom::fill(&mut kid_bytes)
        .map_err(|_| KernelError::Invariant("no randomness for a kid".to_owned()))?;
    Ok(MintedKey {
        kid: B64.encode(kid_bytes),
        modulus,
        exponent,
        sealed: seal.seal(der.as_bytes())?,
    })
}

/// The two base64url members a JWK publishes an RSA public key as.
#[must_use]
pub fn jwk_parts(public: &RsaPublicKey) -> (String, String) {
    (
        B64.encode(public.n().to_bytes_be()),
        B64.encode(public.e().to_bytes_be()),
    )
}

/// Rebuild a public key from the two members a row stores, so a verifier
/// works from the same bytes the JWKS published.
pub fn public_of(row: &KeyRow) -> Outcome<RsaPublicKey> {
    let n = B64
        .decode(&row.modulus)
        .map_err(|_| KernelError::Invariant("a key row's modulus is not base64url".to_owned()))?;
    let e = B64
        .decode(&row.exponent)
        .map_err(|_| KernelError::Invariant("a key row's exponent is not base64url".to_owned()))?;
    RsaPublicKey::new(
        rsa::BigUint::from_bytes_be(&n),
        rsa::BigUint::from_bytes_be(&e),
    )
    .map_err(|_| KernelError::Invariant("a key row is not an RSA public key".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealing_key_reads_back_from_its_hex_and_prints_nothing() {
        let key = SealKey::mint();
        let hex = key.to_hex();
        assert_eq!(hex.len(), 64);
        let back = SealKey::from_hex(&hex).expect("its own hex");
        assert_eq!(back.0, key.0);
        assert_eq!(format!("{key:?}"), "SealKey(..)");
        assert!(!format!("{key:?}").contains(&hex));
        assert!(SealKey::from_hex("too short").is_none());
        assert!(SealKey::from_hex(&"z".repeat(64)).is_none());
    }

    #[test]
    fn a_private_half_opens_only_under_the_key_that_sealed_it() {
        let key = SealKey::mint();
        let sealed = key.seal(b"the private half").expect("it seals");
        assert_ne!(&sealed[12..], b"the private half");
        assert_eq!(key.open(&sealed).expect("it opens"), b"the private half");
        assert!(SealKey::mint().open(&sealed).is_err(), "a different key");
        assert!(key.open(&sealed[..8]).is_err(), "a truncated ciphertext");
    }

    #[test]
    fn a_minted_key_round_trips_through_the_columns_a_row_holds() {
        let seal = SealKey::mint();
        let minted = mint(&seal).expect("a signing key");
        let row = KeyRow {
            id: Id::new(1),
            kid: minted.kid.clone(),
            status: "active".to_owned(),
            modulus: minted.modulus.clone(),
            exponent: minted.exponent.clone(),
            created_at: 0,
        };
        let public = public_of(&row).expect("the public half");
        let der = seal.open(&minted.sealed).expect("the private half opens");
        let private = RsaPrivateKey::from_pkcs8_der(&der).expect("a private key");
        assert_eq!(RsaPublicKey::from(&private), public);
        assert_eq!(minted.exponent, "AQAB", "65537, as every RSA key uses");
    }
}
