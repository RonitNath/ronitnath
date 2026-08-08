//! Email factor helpers.

use tracing::warn;

use super::model::IdentityEmail;

/// Gate that will eventually require `verified_at`.
///
/// Verification is not implemented yet: the column exists, the check is
/// called on the authenticate path, and for now it logs and lets the
/// request continue so registration can ship without outbound mail.
pub fn ensure_email_verified(email: &IdentityEmail) -> Result<(), EmailVerifyError> {
    if email.verified_at.is_some() {
        return Ok(());
    }

    warn!(
        email_id = email.id.get(),
        identity_id = email.identity_id.get(),
        "email verification is not implemented yet; skipping verified_at check"
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailVerifyError {
    /// Reserved for when verification is enforced.
    Unverified,
}

impl std::fmt::Display for EmailVerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unverified => write!(f, "email address is not verified"),
        }
    }
}

impl std::error::Error for EmailVerifyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ids::InternalId;
    use crate::auth::model::IdentityEmail;

    fn sample(verified_at: Option<i64>) -> IdentityEmail {
        IdentityEmail {
            id: InternalId::new(1),
            identity_id: InternalId::new(2),
            email: "a@b.co".into(),
            email_normalized: "a@b.co".into(),
            verified_at,
            is_primary: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn verified_email_passes() {
        assert!(ensure_email_verified(&sample(Some(1))).is_ok());
    }

    #[test]
    fn unverified_email_currently_skipped() {
        // Will become Err(Unverified) once verification lands.
        assert!(ensure_email_verified(&sample(None)).is_ok());
    }
}
