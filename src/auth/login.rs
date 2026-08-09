//! Authenticate-path gates (email verify, password check).

use std::time::Instant;

use tracing::{debug, info, instrument};

use super::email::ensure_email_verified;
use super::model::IdentityEmail;
use super::password::{dummy_phc, verify_password};

/// Run the shared pre-session gates and report whether the credential matched.
///
/// - If `email_row` is present, run the email-verification gate (currently
///   warns and skips when `verified_at` is unset).
/// - Always verify against `stored_phc`, or a dummy PHC when the address is
///   unknown, so timing does not enumerate accounts.
///
/// The returned `bool` is *only* the password verdict. It is deliberately not
/// enough on its own to sign anyone in: an unknown address also produces
/// `false` here after the same work, and the caller still has to establish that
/// the identity and its account are active. Callers must not surface which of
/// those checks failed.
#[instrument(name = "auth.run_gates", skip(email_row, password, stored_phc), fields(known_email = email_row.is_some()))]
pub fn run_auth_gates(
    email_row: Option<&IdentityEmail>,
    password: &str,
    stored_phc: Option<&str>,
) -> bool {
    let started = Instant::now();
    if let Some(row) = email_row {
        let _ = ensure_email_verified(row);
    }
    let phc = stored_phc.unwrap_or_else(|| dummy_phc());
    let matched = verify_password(password, phc).unwrap_or(false);
    // Never true for an unknown address: the dummy PHC has no known preimage.
    let matched = matched && stored_phc.is_some();
    debug!(matched, "password gate finished");
    info!(
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "auth gates finished"
    );
    matched
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::password::hash_password;

    #[test]
    fn unknown_address_never_matches() {
        assert!(!run_auth_gates(None, "anything", None));
    }

    #[test]
    fn known_address_matches_only_the_right_password() {
        let phc = hash_password("correct horse battery staple").unwrap();
        assert!(run_auth_gates(
            None,
            "correct horse battery staple",
            Some(&phc)
        ));
        assert!(!run_auth_gates(None, "wrong", Some(&phc)));
    }
}
