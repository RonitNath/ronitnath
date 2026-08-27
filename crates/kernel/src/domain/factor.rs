//! `factor` — what an identity has proven.

use super::Vocabulary;
use crate::Timestamp;
use crate::ids::{Factor, Id, Identity};
use crate::store::{Cursor, FromRow, RowError};

/// What a factor proves.
///
/// All four are admitted by the column's CHECK; only `email` and `password`
/// are produced by a command in this cut. Admitting the other two now is not
/// speculation — it is the difference between a passkey landing as code and a
/// passkey landing as code plus a migration on a live cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FactorKind {
    /// An address. `value` is the normalised address; `verified_at` is set by
    /// `VerifyEmail`.
    Email,
    /// A password. `value` is an argon2id PHC string and nothing else.
    Password,
    /// A WebAuthn credential. Schema only.
    Passkey,
    /// An OIDC subject at an external issuer. Schema only.
    Oidc,
}

impl Vocabulary for FactorKind {
    const ALL: &'static [Self] = &[Self::Email, Self::Password, Self::Passkey, Self::Oidc];
    const COLUMN: (&'static str, &'static str) = ("factor", "kind");

    fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Password => "password",
            Self::Passkey => "passkey",
            Self::Oidc => "oidc",
        }
    }
}

impl FactorKind {
    /// Whether a command in this cut can produce this kind.
    pub const fn is_built(self) -> bool {
        matches!(self, Self::Email | Self::Password)
    }
}

impl From<rn_api::commands::FactorKind> for FactorKind {
    fn from(kind: rn_api::commands::FactorKind) -> Self {
        match kind {
            rn_api::commands::FactorKind::Email => Self::Email,
            rn_api::commands::FactorKind::Password => Self::Password,
            rn_api::commands::FactorKind::Passkey => Self::Passkey,
            rn_api::commands::FactorKind::Oidc => Self::Oidc,
        }
    }
}

impl From<FactorKind> for rn_api::commands::FactorKind {
    fn from(kind: FactorKind) -> Self {
        match kind {
            FactorKind::Email => Self::Email,
            FactorKind::Password => Self::Password,
            FactorKind::Passkey => Self::Passkey,
            FactorKind::Oidc => Self::Oidc,
        }
    }
}

/// A `factor` row.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorRow {
    /// The rowid.
    pub id: Id<Factor>,
    /// The identity it belongs to.
    pub identity_id: Id<Identity>,
    /// What it proves.
    pub kind: FactorKind,
    /// The address, or the PHC string. Never leaves the process.
    pub value: String,
    /// When it was proven, if it has been.
    pub verified_at: Option<Timestamp>,
}

impl FactorRow {
    /// The columns this row reads.
    pub const COLUMNS: &'static str = "id, identity_id, kind, value, verified_at";

    /// Whether this factor has been proven.
    pub const fn is_verified(&self) -> bool {
        self.verified_at.is_some()
    }
}

impl FromRow for FactorRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        let kind = row.text("kind")?;
        Ok(Self {
            id: row.id("id")?,
            identity_id: row.id("identity_id")?,
            kind: FactorKind::parse(&kind).ok_or(RowError::Column {
                column: "kind".to_owned(),
                wanted: "FactorKind",
            })?,
            value: row.text("value")?,
            verified_at: row.int_opt("verified_at")?,
        })
    }
}
