//! Bearer secrets: session cookies and link tokens.
//!
//! Both are 256 random bits from the OS, handed out once as base64url and
//! stored only as their SHA-256. Nothing derives them from a row, so a
//! database read — a backup, a replica, a snapshot on someone's laptop —
//! cannot mint a session or claim a link.
//!
//! [`Token`] holds the secret and refuses to print it. That is not decoration:
//! the reason tokens end up in logs is that some struct three layers away
//! derived `Debug`.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use sha2::{Digest, Sha256};

/// Bits of entropy in a bearer secret.
const TOKEN_BYTES: usize = 32;

/// A bearer secret, in the one moment it exists in the clear.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// Mint a fresh 256-bit secret.
    ///
    /// # Panics
    ///
    /// If the operating system cannot produce randomness. There is no sound
    /// way to continue from that: every alternative mints a guessable session.
    pub fn mint() -> Self {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes).expect("the operating system must provide randomness");
        Self(B64.encode(bytes))
    }

    /// Wrap a secret that arrived from a caller.
    pub fn from_wire(raw: &str) -> Self {
        Self(raw.trim().to_owned())
    }

    /// The secret, for the one place that sends it: the reply that mints it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// What the row stores.
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        hasher.finalize().into()
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(redacted)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_never_prints_itself() {
        let token = Token::mint();
        let printed = format!("{token:?}");
        assert_eq!(printed, "Token(redacted)");
        assert!(!printed.contains(token.expose()));
    }

    #[test]
    fn two_tokens_are_never_the_same() {
        let a = Token::mint();
        let b = Token::mint();
        assert_ne!(a.expose(), b.expose());
        assert_eq!(a.expose().len(), 43, "256 bits, base64url, unpadded");
    }

    #[test]
    fn the_digest_is_not_the_token() {
        let token = Token::mint();
        let digest = token.digest();
        assert_ne!(digest.as_slice(), token.expose().as_bytes());
        assert_eq!(digest, token.digest(), "and it is stable");
        assert_ne!(digest, Token::mint().digest());
    }
}
