//! Authentication and authorization.
//!
//! The model — identity (who's acting) ≠ account (who owns the data) ≠
//! factor (how they proved it) — is documented in full in
//! `docs/plans/2026-07-stage2-hardened-fork-template.md`. This module is
//! organized by concern rather than by table:
//!
//! - [`password`] / [`api_token`] — the two phase-1 login mechanisms.
//!   Later phases add `oidc`/`magic_link`/`qr_device` here behind a
//!   `FactorKind` trait (see the plan doc) — not introduced yet since
//!   there's only one synchronous mechanism (`password`) to generalize
//!   from; a trait with a single implementation is just ceremony.
//! - [`session`] — cookie plumbing (token generation, hashing).
//! - [`csrf`] — the synchronizer-token check for mutating requests.
//! - [`middleware`] — resolves the session cookie once per request.
//! - [`extract`] — [`AccountScope`], the extractor handlers actually use.
//! - [`login`] — signup/login/logout business logic, and the audit calls
//!   that go with them.

pub mod api_token;
pub mod csrf;
pub mod extract;
pub mod login;
pub mod middleware;
pub mod password;
pub mod session;

pub use extract::AccountScope;

/// A membership's role on an account. Ordered so `role >= Role::Admin`
/// reads naturally in a gate check — declaration order is the ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Member,
    Admin,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Self {
        match s {
            "owner" => Role::Owner,
            "admin" => Role::Admin,
            _ => Role::Member,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::Member => "member",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
