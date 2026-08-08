//! Password providence factor — argon2id PHC strings.

use std::sync::LazyLock;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::password_hash::rand_core::OsRng;
use argon2::{Algorithm, Argon2, Params, Version};

/// OWASP recommended argon2id parameters (m=19 MiB, t=2, p=1).
fn argon2() -> Argon2<'static> {
    let params = Params::new(19_456, 2, 1, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Hash a password into a PHC string for `identity_passwords.argon2_phc`.
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| PasswordError::Hash(e.to_string()))?;
    Ok(hash.to_string())
}

/// Constant-time verify against a stored PHC string.
pub fn verify_password(password: &str, phc: &str) -> Result<bool, PasswordError> {
    let parsed = PasswordHash::new(phc).map_err(|e| PasswordError::Hash(e.to_string()))?;
    Ok(argon2()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

static DUMMY_PHC: LazyLock<String> = LazyLock::new(|| {
    hash_password("rn-site-dummy-password-not-a-real-credential")
        .expect("dummy argon2id hash")
});

/// Valid PHC used to equalize timing when the email is unknown.
#[must_use]
pub fn dummy_phc() -> &'static str {
    DUMMY_PHC.as_str()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordError {
    Hash(String),
}

impl std::fmt::Display for PasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hash(msg) => write!(f, "password hash error: {msg}"),
        }
    }
}

impl std::error::Error for PasswordError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_round_trip() {
        let phc = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &phc).unwrap());
        assert!(!verify_password("wrong", &phc).unwrap());
    }

    #[test]
    fn dummy_phc_parses() {
        assert!(PasswordHash::new(dummy_phc()).is_ok());
    }
}
