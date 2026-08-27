//! Passwords: argon2id, and a decline that takes the same time either way.
//!
//! The parameters are argon2's own defaults for the id variant — 19 MiB, three
//! passes, one lane — which is the OWASP recommendation and, more usefully,
//! the number this crate will not quietly drift away from, because it is the
//! library's default rather than a constant somebody chose once.
//!
//! ## Why the dummy verify
//!
//! Signing in with an unknown address costs one indexed lookup. Signing in
//! with a known address costs that plus an argon2 verification — tens of
//! milliseconds. Left alone, the difference is an oracle: a caller learns
//! which addresses are registered by timing the refusal, which is exactly the
//! enumeration the uniform [`Decline`](crate::Decline) exists to prevent. So
//! an unknown address is verified against a fixed hash whose password nobody
//! has, and both paths pay the same.
//!
//! ## Why the blocking variants exist
//!
//! Nineteen mebibytes and three passes is tens of milliseconds of deliberate
//! CPU. Spent on a runtime worker it is not this request that suffers but
//! every other connection the node is serving — including the 25-second
//! heartbeat on `/api/sub`, which makes it a correctness question rather than
//! a tuning one. [`hash_blocking`], [`verify_blocking`] and
//! [`verify_dummy_blocking`] put exactly the KDF on tokio's blocking pool and
//! nothing else: the command's transaction, its audit read-back and its feed
//! notify stay on the reactor where they belong.

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

/// A PHC string of a password nobody holds, for the timing-uniform decline.
///
/// It is a real hash of a real string, so verification does the real work; the
/// string is a constant of this file and authenticates nothing, because no
/// factor row carries this hash.
const DUMMY_PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0c2E$\
                         5uUdRsw2sSPBWUGTeeIQeVvHRvZbcYFvGe9ZDrRDLho";

/// Why a password could not be hashed. Never a caller's fault.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("password hashing: {0}")]
pub struct HashError(String);

/// Hash a password for storage.
pub fn hash(password: &str) -> Result<String, HashError> {
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|phc| phc.to_string())
        .map_err(|err| HashError(err.to_string()))
}

/// Check a password against a stored PHC string.
///
/// A malformed stored hash verifies as `false` rather than erroring: it is a
/// row that cannot authenticate anybody, which is a decline, and turning it
/// into a distinguishable failure would be another oracle.
#[must_use]
pub fn verify(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Hash a password on the blocking pool.
///
/// The same work as [`hash`], off the reactor. A pool thread that panics or is
/// cancelled reads as a hashing failure, which is an invariant violation and
/// never a caller's fault.
pub async fn hash_blocking(password: &str) -> Result<String, HashError> {
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || hash(&password))
        .await
        .unwrap_or_else(|joined| Err(HashError(joined.to_string())))
}

/// Check a password on the blocking pool.
///
/// A pool thread that did not finish verifies as `false`, on the same
/// reasoning as a malformed stored hash: it authenticated nobody.
#[must_use]
pub async fn verify_blocking(password: &str, phc: &str) -> bool {
    let (password, phc) = (password.to_owned(), phc.to_owned());
    tokio::task::spawn_blocking(move || verify(&password, &phc))
        .await
        .unwrap_or(false)
}

/// Spend the same work as a real verification, on the blocking pool, and
/// always refuse. The timing-uniform decline, off the reactor.
pub async fn verify_dummy_blocking(password: &str) {
    let _ = verify_blocking(password, DUMMY_PHC).await;
}

/// Spend the same work as a real verification, and always refuse.
///
/// Called on the path where no factor was found, so a caller cannot tell an
/// unknown address from a wrong password by how long the refusal took.
pub fn verify_dummy(password: &str) {
    let _ = verify(password, DUMMY_PHC);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_verifies_against_its_own_hash_and_nothing_else() {
        let phc = hash("correct horse battery staple").expect("hashes");
        assert!(phc.starts_with("$argon2id$"), "{phc}");
        assert!(verify("correct horse battery staple", &phc));
        assert!(!verify("Correct horse battery staple", &phc));
        assert!(!verify("", &phc));
    }

    #[test]
    fn two_hashes_of_one_password_differ() {
        let a = hash("the same password").expect("hashes");
        let b = hash("the same password").expect("hashes");
        assert_ne!(a, b, "each hash carries its own salt");
        assert!(verify("the same password", &a) && verify("the same password", &b));
    }

    #[test]
    fn a_hash_that_is_not_a_hash_authenticates_nobody() {
        assert!(!verify("anything", "not a PHC string"));
        assert!(!verify("anything", ""));
    }

    #[test]
    fn the_dummy_hash_is_well_formed_so_the_decline_does_the_real_work() {
        assert!(
            PasswordHash::new(DUMMY_PHC).is_ok(),
            "a malformed dummy would return early and reintroduce the oracle"
        );
        verify_dummy("whatever");
    }

    #[tokio::test]
    async fn a_hash_taken_off_the_reactor_is_the_same_hash() {
        let phc = hash_blocking("correct horse battery staple")
            .await
            .expect("hashes");
        assert!(phc.starts_with("$argon2id$"), "{phc}");
        // Hashed on one pool thread, verified on another, and verified by the
        // synchronous path too: the blocking variants are the same function.
        assert!(verify_blocking("correct horse battery staple", &phc).await);
        assert!(verify("correct horse battery staple", &phc));
        assert!(!verify_blocking("Correct horse battery staple", &phc).await);
        verify_dummy_blocking("whatever").await;
    }

    /// The timing property itself, measured coarsely: an unknown-address
    /// decline must not be an order of magnitude cheaper than a known one.
    /// A tight bound would be flaky on a shared machine; an order of magnitude
    /// is what an attacker can actually measure over a network.
    #[test]
    fn refusing_an_unknown_address_costs_what_refusing_a_wrong_password_costs() {
        let phc = hash("a real password").expect("hashes");
        let started = std::time::Instant::now();
        assert!(!verify("wrong", &phc));
        let known = started.elapsed();

        let started = std::time::Instant::now();
        verify_dummy("wrong");
        let unknown = started.elapsed();

        assert!(
            unknown * 10 > known,
            "unknown {unknown:?} is more than 10x cheaper than known {known:?}"
        );
    }
}
