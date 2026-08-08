//! Authenticate-path gates (email verify, password check).

use std::time::Instant;

use tracing::{info, instrument};

use super::email::ensure_email_verified;
use super::model::IdentityEmail;
use super::password::{dummy_phc, verify_password};

/// Run the shared pre-session gates.
///
/// - If `email_row` is present, run the email-verification gate (currently
///   warns and skips when `verified_at` is unset).
/// - Always verify against `stored_phc`, or a dummy PHC when the address is
///   unknown, so timing does not enumerate accounts.
///
/// Does not issue a session; the caller decides success vs the generic
/// decline after these gates return.
#[instrument(name = "auth.run_gates", skip(email_row, password, stored_phc), fields(known_email = email_row.is_some()))]
pub fn run_auth_gates(
    email_row: Option<&IdentityEmail>,
    password: &str,
    stored_phc: Option<&str>,
) {
    let started = Instant::now();
    if let Some(row) = email_row {
        let _ = ensure_email_verified(row);
    }
    let phc = stored_phc.unwrap_or_else(|| dummy_phc());
    let _ = verify_password(password, phc);
    info!(
        latency_ms = started.elapsed().as_secs_f64() * 1000.0,
        "auth gates finished"
    );
}
