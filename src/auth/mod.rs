//! Identity, accounts, credentials, and sessions.
//!
//! Schema lives in `migrations/1_auth.sql`. Integer `id` never leaves the
//! server; UUID v4 `public_id` is resolved through [`ids::PublicIdIndex`].

mod email;
mod ids;
mod login;
mod model;
mod password;

pub use email::{EmailVerifyError, ensure_email_verified};
pub use ids::{InternalId, PublicId, PublicIdIndex, PublicIdKind};
pub use login::run_auth_gates;
pub use model::{
    Account, AccountKind, AccountMembership, AccountStatus, Identity, IdentityEmail, IdentityKind,
    IdentityStatus, MembershipRole, Session, normalize_email,
};
pub use password::{PasswordError, dummy_phc, hash_password, verify_password};

/// Embedded hiqlite migrations (`migrations/*.sql`).
#[derive(rust_embed::Embed)]
#[folder = "migrations"]
pub struct Migrations;
