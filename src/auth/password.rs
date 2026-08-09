//! Password providence factor — argon2id PHC strings.

use std::sync::LazyLock;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

// Current write policy. These values affect only newly-created hashes;
// verification reconstructs its algorithm, version, parameters, salt, and
// output length from each stored PHC string.
const CURRENT_M_COST_KIB: u32 = 19_456;
const CURRENT_T_COST: u32 = 2;
const CURRENT_P_COST: u32 = 1;
const CURRENT_OUTPUT_LEN: usize = 32;
#[cfg(test)]
const CURRENT_VERSION: u32 = 19;

/// Argon2id v19 using the current OWASP-recommended policy.
fn current_argon2() -> Argon2<'static> {
    let params = Params::new(
        CURRENT_M_COST_KIB,
        CURRENT_T_COST,
        CURRENT_P_COST,
        Some(CURRENT_OUTPUT_LEN),
    )
    .expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Hash a password into a complete PHC string for `identity_passwords.password_hash`.
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = current_argon2()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| PasswordError::Hash(e.to_string()))?;
    Ok(hash.to_string())
}

/// Constant-time verify using the policy encoded in the stored PHC string.
pub fn verify_password(password: &str, phc: &str) -> Result<bool, PasswordError> {
    let parsed = PasswordHash::new(phc).map_err(|e| PasswordError::Hash(e.to_string()))?;
    Ok(current_argon2()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

static DUMMY_PHC: LazyLock<String> = LazyLock::new(|| {
    hash_password("rn-site-dummy-password-not-a-real-credential").expect("dummy argon2id hash")
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
    fn historical_hash_uses_its_embedded_policy() {
        // RustCrypto's Argon2id v19 test vector for "password". Its 256 KiB
        // memory cost differs from the current write policy, so this will only
        // verify if `verify_password` honors the stored PHC parameters.
        const HISTORICAL_PHC: &str =
            "$argon2id$v=19$m=256,t=2,p=1$c29tZXNhbHQ$nf65EOgLrQMR/uIPnA4rEsF5h7TKyQwu9U1bMCHGi/4";

        assert!(verify_password("password", HISTORICAL_PHC).unwrap());
        assert!(!verify_password("not-password", HISTORICAL_PHC).unwrap());
    }

    #[test]
    fn new_hash_contains_the_complete_current_policy() {
        let phc = hash_password("correct horse battery staple").unwrap();
        let parsed = PasswordHash::new(&phc).unwrap();

        assert_eq!(parsed.algorithm.as_str(), "argon2id");
        assert_eq!(parsed.version, Some(CURRENT_VERSION));
        assert_eq!(parsed.params.get_decimal("m").unwrap(), CURRENT_M_COST_KIB);
        assert_eq!(parsed.params.get_decimal("t").unwrap(), CURRENT_T_COST);
        assert_eq!(parsed.params.get_decimal("p").unwrap(), CURRENT_P_COST);

        let mut salt = [0_u8; 64];
        assert_eq!(
            parsed.salt.unwrap().decode_b64(&mut salt).unwrap().len(),
            16
        );
        assert_eq!(parsed.hash.unwrap().len(), CURRENT_OUTPUT_LEN);
    }

    #[test]
    fn dummy_phc_parses() {
        assert!(PasswordHash::new(dummy_phc()).is_ok());
    }
}
