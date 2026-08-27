//! The rows the kernel owns, and the vocabularies their status columns carry.
//!
//! Every enum here has a `&'static str` form that is exactly one of the values
//! its column's CHECK constraint admits, and a parse that refuses anything
//! else. The two halves are held together by a test per vocabulary that reads
//! the constraint out of `sqlite_master` and compares — so adding a status to
//! the Rust enum without adding it to the schema fails, and so does the
//! reverse.

mod factor;
mod identity;
mod link;
mod party;
mod session;
#[cfg(test)]
mod tests;
mod token;

pub use factor::{FactorKind, FactorRow};
pub use identity::{IdentityRow, IdentityStatus};
pub use link::{LinkRow, VERIFY_TTL};
pub use party::{PartyKind, PartyRow, PartyStatus};
pub use session::{RENEW_AFTER, SESSION_TTL, SessionRow};
pub use token::Token;

/// Longest a display name may be. Long enough for a legal entity's name,
/// short enough that a row cannot become a payload.
pub const DISPLAY_NAME_LIMIT: usize = 200;

/// Longest an email address may be — RFC 5321's practical ceiling.
pub const EMAIL_LIMIT: usize = 254;

/// Shortest password accepted. A floor, not a policy: length is the only
/// requirement that survives contact with how people actually choose one.
pub const PASSWORD_MIN: usize = 12;

/// The zone a row lives in. One zone exists today; the column is what rung 10
/// grows into and what keeps every query already written in terms of it.
pub const DEFAULT_ZONE: &str = "us-west";

/// The source an identity registered at. Source-scoped from the start, so
/// "imported from AcquiredCo" needs no migration.
pub const DEFAULT_SOURCE: &str = "rn-site";

/// A vocabulary that is also a CHECK constraint.
pub trait Vocabulary: Sized + Copy + PartialEq + 'static {
    /// Every value, in the order the constraint lists them.
    const ALL: &'static [Self];
    /// The table and column the constraint is on, for the schema agreement
    /// test.
    const COLUMN: (&'static str, &'static str);

    /// The value as the column stores it.
    fn as_str(self) -> &'static str;

    /// Read a value back, refusing anything the constraint would have.
    fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.as_str() == raw)
    }
}

/// Normalise an email address for comparison: trimmed and lowercased.
///
/// The local part is case-sensitive by the RFC and case-insensitive at every
/// provider anyone uses; treating it as insensitive is what stops one human
/// registering twice by holding shift.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Whether a string is plausibly an address. Deliberately shallow: the only
/// real check is sending a message to it, which [`VerifyEmail`] does.
///
/// [`VerifyEmail`]: crate::cmd::verify_email
pub fn looks_like_email(raw: &str) -> bool {
    let mut parts = raw.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !raw.chars().any(char::is_whitespace)
}
