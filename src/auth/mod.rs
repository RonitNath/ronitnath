//! Identity, accounts, credentials, capabilities, and sessions.
//!
//! Schema lives in `migrations/1_auth.sql`. Integer `id` never leaves the
//! server; UUID v4 `public_id` is resolved through [`ids::PublicIdIndex`].
//!
//! The request path is: [`routes`] takes the credential, [`store`] talks to
//! hiqlite, [`session`] mints and reads the cookie, and [`guard`] turns that
//! cookie into a [`capability`] check in front of a page.

pub mod capability;
mod email;
pub mod grants;
pub mod guard;
mod ids;
mod login;
mod model;
mod password;
pub mod routes;
pub mod session;
pub mod store;

pub use capability::{Capability, CapabilitySet};
pub use email::{EmailVerifyError, ensure_email_verified};
pub use grants::{Access, GrantSubject, ResourceAction, ResourceKind, ShareRole};
pub use ids::{InternalId, PublicId, PublicIdIndex, PublicIdKind};
pub use login::run_auth_gates;
pub use model::{
    Account, AccountKind, AccountMembership, AccountStatus, Identity, IdentityEmail, IdentityKind,
    IdentityStatus, MembershipRole, Session, normalize_email,
};
pub use password::{PasswordError, dummy_phc, hash_password, verify_password};
pub use routes::{api_router, router};
pub use session::CookieSecurity;
pub use store::{Registration, SessionContext, StoreError};

/// What every auth route and the guard need from the process.
///
/// Cloned per request by axum; `Client` is already reference-counted internally.
#[derive(Clone)]
pub struct AuthState {
    pub db: hiqlite::Client,
    /// Whether session cookies are marked `Secure`. See [`CookieSecurity`].
    pub cookie_security: CookieSecurity,
    /// Runtime mode. Auth behavior that must never exist in prod — the dev
    /// bypass — checks this at request time, on top of its compile-time gate.
    pub mode: crate::operations::config::Mode,
}

impl AuthState {
    /// Build with an explicit cookie policy and mode.
    ///
    /// The policy is a parameter and not a default because the failure mode of
    /// getting it wrong is invisible: an insecure cookie in prod works fine
    /// right up until it is sent over plaintext. Prefer [`Self::for_mode`].
    #[must_use]
    pub fn new(
        db: hiqlite::Client,
        cookie_security: CookieSecurity,
        mode: crate::operations::config::Mode,
    ) -> Self {
        Self {
            db,
            cookie_security,
            mode,
        }
    }

    /// Build with the cookie policy the runtime mode implies: `Secure` in prod.
    #[must_use]
    pub fn for_mode(db: hiqlite::Client, mode: crate::operations::config::Mode) -> Self {
        Self::new(db, CookieSecurity::for_mode(mode), mode)
    }
}

impl std::fmt::Debug for AuthState {
    /// Hand-written so a `?state` in a log line can never print the secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthState")
            .field("cookie_security", &self.cookie_security)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

/// Embedded hiqlite migrations (`migrations/*.sql`).
#[derive(rust_embed::Embed)]
#[folder = "migrations"]
pub struct Migrations;
