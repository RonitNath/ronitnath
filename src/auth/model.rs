//! Durable auth entities. Integer ids stay server-local; `public_id` is UUID v4.

use super::ids::{InternalId, PublicId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityKind {
    Person,
    Service,
}

impl IdentityKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Service => "service",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityStatus {
    Pending,
    Active,
    Disabled,
}

impl IdentityStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }
}

/// Account ownership boundary.
///
/// `primary` is the default account created with an identity (formerly
/// "personal"). The other kinds — business, shared, alternate, service —
/// are additional ownership boundaries an identity may join.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountKind {
    Primary,
    Business,
    Shared,
    Alternate,
    Service,
}

impl AccountKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Business => "business",
            Self::Shared => "shared",
            Self::Alternate => "alternate",
            Self::Service => "service",
        }
    }
}

impl std::str::FromStr for AccountKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "primary" => Ok(Self::Primary),
            "business" => Ok(Self::Business),
            "shared" => Ok(Self::Shared),
            "alternate" => Ok(Self::Alternate),
            "service" => Ok(Self::Service),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    Disabled,
    Suspended,
}

impl AccountStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Suspended => "suspended",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MembershipRole {
    Owner,
    Admin,
    Member,
}

impl MembershipRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }
}

impl std::str::FromStr for MembershipRole {
    type Err = ();

    /// Parse the stored form. The schema `CHECK`s the column to these three
    /// values, so an `Err` here means the row and the code disagree about the
    /// schema — the caller should treat it as corruption, not as a default.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Identity {
    pub id: InternalId,
    pub public_id: PublicId,
    pub kind: IdentityKind,
    pub display_name: Option<String>,
    pub status: IdentityStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct IdentityEmail {
    pub id: InternalId,
    pub identity_id: InternalId,
    pub email: String,
    pub email_normalized: String,
    /// Unix millis when verified. `None` until verification exists.
    pub verified_at: Option<i64>,
    pub is_primary: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct Account {
    pub id: InternalId,
    pub public_id: PublicId,
    pub kind: AccountKind,
    pub name: String,
    pub status: AccountStatus,
    /// Set iff `kind == Primary`.
    pub primary_for_identity_id: Option<InternalId>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct AccountMembership {
    pub account_id: InternalId,
    pub identity_id: InternalId,
    pub role: MembershipRole,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub id: InternalId,
    pub token_hash: String,
    pub identity_id: InternalId,
    pub account_id: InternalId,
    pub created_at: i64,
    pub expires_at: i64,
    pub last_seen_at: i64,
    pub user_agent: Option<String>,
}

/// Normalize an email for lookup / uniqueness.
#[must_use]
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}
