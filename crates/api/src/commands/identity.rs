//! Identities, factors, sessions, and the status commands that act on a party.

use serde::{Deserialize, Serialize};

use crate::ids::PublicId;

/// What a factor proves. The column exists for all four; only `email` and
/// `password` are accepted by a command in this cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactorKind {
    /// An address, proven by [`VerifyEmail`].
    Email,
    /// A password, stored as an argon2 hash.
    Password,
    /// A WebAuthn credential. Schema only in this cut.
    Passkey,
    /// An OIDC subject at an external issuer. Schema only in this cut.
    Oidc,
}

/// Create an identity with its first factors, in one transaction.
///
/// The identity's `source` is stamped by the server — a registration made here
/// is scoped to this site, and a caller does not get to claim otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Register {
    /// How the new party is named in a UI.
    pub display_name: String,
    /// The email factor, unverified until [`VerifyEmail`].
    pub email: String,
    /// The password factor, hashed before it is stored.
    pub password: String,
}

/// Exchange a password factor for a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignIn {
    /// The email factor identifying the registration.
    pub email: String,
    /// The password factor proving it.
    pub password: String,
}

/// End the session the request arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SignOut {}

/// End some other session of the same identity — signing out a lost device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeSession {
    /// The session to end.
    pub session: PublicId,
}

/// Add a factor to the acting identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFactor {
    /// What the new factor proves.
    pub kind: FactorKind,
    /// Its value: the address, the password, the credential. Never returned.
    pub value: String,
}

/// Remove a factor from the acting identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveFactor {
    /// The factor to remove.
    pub factor: PublicId,
}

/// Prove an email factor with the token that was mailed to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyEmail {
    /// The bearer token from the message. Stored as its SHA-256.
    pub token: String,
}

/// Move a party to `disabled`: it stops authenticating and stops being a
/// usable subject. Reversible with [`Enable`]; distinct from deletion, which
/// this model does not do to a party.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disable {
    /// The party to disable.
    pub party: PublicId,
    /// Why, recorded on the audit row.
    pub reason: String,
}

/// Move a disabled party back to `active`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enable {
    /// The party to re-enable.
    pub party: PublicId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, CommandEnvelope};
    use crate::testing::{id, round_trip};

    #[test]
    fn identity_commands_round_trip() {
        round_trip(&Register {
            display_name: "Ronit".into(),
            email: "ronit@isoastra.com".into(),
            password: "hunter2".into(),
        });
        round_trip(&SignIn {
            email: "ronit@isoastra.com".into(),
            password: "hunter2".into(),
        });
        round_trip(&SignOut {});
        round_trip(&RevokeSession { session: id("s_") });
        round_trip(&AddFactor {
            kind: FactorKind::Passkey,
            value: "credential".into(),
        });
        round_trip(&RemoveFactor { factor: id("f_") });
        round_trip(&VerifyEmail {
            token: "opaque".into(),
        });
        round_trip(&Disable {
            party: id("p_"),
            reason: "operator ruling".into(),
        });
        round_trip(&Enable { party: id("p_") });
    }

    #[test]
    fn a_bodyless_command_still_carries_its_key() {
        let json = serde_json::to_value(CommandEnvelope::new(SignOut {})).expect("serialises");
        assert_eq!(json.as_object().expect("object").len(), 1);
        assert_eq!(SignOut::NAME, "sign-out");
    }
}
