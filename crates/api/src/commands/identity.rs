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

/// Add a factor to an identity of the acting person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFactor {
    /// What the new factor proves.
    pub kind: FactorKind,
    /// Its value: the address, the password, the credential. Never returned.
    pub value: String,
    /// Which identity, when it is not the one the caller signed in with.
    ///
    /// This is the recovery path a merge exists for: after two registrations
    /// are proven to be one human, the person signs in with the one they still
    /// hold and rotates the factors of the one they lost. Omitted, the target
    /// is the acting identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<PublicId>,
}

/// Remove a factor from an identity of the acting person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveFactor {
    /// The factor to remove.
    pub factor: PublicId,
    /// Which identity it belongs to, when it is not the one the caller signed
    /// in with. The other half of the recovery path — see [`AddFactor`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<PublicId>,
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

/// Speak as an organization, or go back to speaking as yourself.
///
/// Attribution, not authority. Switching a session cannot conjure a grant
/// nobody made: what changes is the party an audit row records, which is what
/// makes an organization's history readable as its own rather than as a list
/// of the people who happened to be signed in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActAs {
    /// The organization to speak as. Absent means the person again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub party: Option<PublicId>,
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
        round_trip(&ActAs {
            party: Some(id("o_")),
        });
        round_trip(&ActAs { party: None });
        round_trip(&AddFactor {
            kind: FactorKind::Passkey,
            value: "credential".into(),
            identity: None,
        });
        round_trip(&AddFactor {
            kind: FactorKind::Email,
            value: "recovered@example.test".into(),
            identity: Some(id("i_")),
        });
        round_trip(&RemoveFactor {
            factor: id("f_"),
            identity: None,
        });
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
