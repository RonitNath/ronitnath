//! What a command can refuse, and how much it says while refusing.
//!
//! There are exactly two failures a caller is allowed to tell apart.
//!
//! **[`Decline`]** covers everything about *this* principal and *that* row:
//! not signed in, not permitted, no such thing, wrong password, disabled. One
//! type, one message, because the difference between "no such document" and
//! "not yours" is precisely the fact an attacker is trying to learn. The
//! server renders it as a `403` or a `404` — chosen so browsers behave — with
//! the same body either way.
//!
//! **[`Invalid`]** covers the shape of the request itself: an empty display
//! name, something that is not an email address, a password below the floor.
//! Saying which is safe, because the caller already knows what it sent, and
//! useful, because a form has to render it.
//!
//! [`KernelError::Conflict`] is neither: it is the answer to reusing an
//! idempotency key with a different body, which is a client bug worth naming.

use crate::store::StoreError;

/// The uniform refusal. Carries nothing, deliberately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, thiserror::Error)]
#[error("declined")]
pub struct Decline;

/// The request could not be understood, whoever sent it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Invalid {
    /// A required field was empty or whitespace.
    #[error("{0} is required")]
    Missing(&'static str),
    /// A field was longer than the schema will carry.
    #[error("{field} is longer than {limit} characters")]
    TooLong {
        /// Which field.
        field: &'static str,
        /// The limit it passed.
        limit: usize,
    },
    /// Not an email address.
    #[error("that is not an email address")]
    NotAnEmail,
    /// A password below the floor. The floor is stated so a form can too.
    #[error("a password is at least {0} characters")]
    PasswordTooShort(usize),
    /// A factor kind the schema admits but no command in this cut produces.
    #[error("that factor kind is not accepted yet")]
    UnsupportedFactor,
}

/// Everything a command returns instead of an event.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KernelError {
    /// Not permitted, or not there. Which one is not disclosed.
    #[error(transparent)]
    Decline(#[from] Decline),
    /// The request itself is malformed.
    #[error(transparent)]
    Invalid(#[from] Invalid),
    /// The idempotency key has been used before, for a different body.
    #[error("that idempotency key was used for a different request")]
    Conflict,
    /// The database refused, or broke.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// An invariant this crate is supposed to maintain did not hold. Never a
    /// caller's fault and never useful to a caller, but it must not be
    /// silently folded into [`Self::Decline`]: that would hide a bug behind a
    /// perfectly ordinary-looking `403`.
    #[error("kernel invariant: {0}")]
    Invariant(String),
}

/// What a command returns.
pub type Outcome<T> = Result<T, KernelError>;

/// Refuse, uniformly.
pub fn decline<T>() -> Outcome<T> {
    Err(KernelError::Decline(Decline))
}

impl KernelError {
    /// Whether this is the uniform refusal — the only thing a caller outside
    /// the server should branch on.
    pub const fn is_decline(&self) -> bool {
        matches!(self, Self::Decline(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_decline_reads_the_same_however_it_arose() {
        let not_found = KernelError::Decline(Decline);
        let not_yours = KernelError::from(Decline);
        assert_eq!(not_found, not_yours);
        assert_eq!(not_found.to_string(), "declined");
        assert!(not_found.is_decline());
    }

    #[test]
    fn a_validation_failure_says_what_is_wrong_because_the_caller_sent_it() {
        assert_eq!(
            Invalid::PasswordTooShort(12).to_string(),
            "a password is at least 12 characters"
        );
        assert_eq!(
            Invalid::TooLong {
                field: "display name",
                limit: 200
            }
            .to_string(),
            "display name is longer than 200 characters"
        );
        assert!(!KernelError::from(Invalid::NotAnEmail).is_decline());
    }
}
