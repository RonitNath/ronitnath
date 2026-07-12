//! The `password` factor: argon2id hashing/verification.

use std::sync::LazyLock;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::OsRng;

/// Hashed once at first use and reused for every "unknown email" login
/// attempt, so verifying against a real hash and verifying against this
/// one take the same amount of time — an attacker measuring response
/// latency can't use it to enumerate which emails have accounts.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash("not-a-real-password-just-for-timing").expect("hashing a fixed string cannot fail"));

#[cfg(test)]
static DUMMY_VERIFY_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub fn dummy_verify_count() -> usize {
    DUMMY_VERIFY_COUNT.load(Ordering::Relaxed)
}

pub fn hash(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

/// Verifies `password` against `hash`, or — if `hash` is `None` (no
/// account for this email) — against the dummy hash, so the two cases
/// take indistinguishable time.
pub fn verify(password: &str, hash: Option<&str>) -> bool {
    #[cfg(test)]
    if hash.is_none() {
        DUMMY_VERIFY_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    let hash = hash.unwrap_or(&DUMMY_HASH);
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}
