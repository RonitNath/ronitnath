//! The platform operator's own commands: delegation, re-authentication,
//! impersonation, and retiring a signing key.
//!
//! Every one of them carries a mandatory `reason`. That is not ceremony. Each
//! is an act with no other actor to check it — a second operator, a session
//! minted for somebody who did not ask for it, a key withdrawn from the JWKS —
//! and the only thing that makes such an act reviewable later is that the
//! person who did it had to say why at the time. The bound is `RuleMatch`'s:
//! long enough for the paragraph an operator should write, short enough that
//! the column is not a document store.

use serde::{Deserialize, Serialize};

use crate::ids::PublicId;

/// Grant `platform:* #operator` to a person.
///
/// Not the bootstrap. The bootstrap creates the *first* operator out of
/// nothing and writes no audit row, because there is nobody yet to attribute
/// it to; it declines the moment one exists. This one has an actor, so it is a
/// command like any other, and the row it writes carries who granted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantOperator {
    /// Who becomes an operator.
    pub person: PublicId,
    /// Why. Mandatory: a delegation of full authority with no stated reason is
    /// a row nobody can account for afterwards.
    pub reason: String,
}

/// Take `platform:* #operator` away.
///
/// Revoking your own is allowed while another operator exists. Revoking the
/// last one is refused — the same shape as the last-owner rule, and for the
/// same reason: a deployment with no operator is one nobody can grant the
/// first of again, because the bootstrap only fires on an empty one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeOperator {
    /// Whose operator relation goes.
    pub person: PublicId,
    /// Why.
    pub reason: String,
}

/// Present the password again, on the session that is already signed in.
///
/// No new session row and no new cookie: what moves is `session.auth_time`,
/// which is what the sensitive commands measure their window against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReAuthenticate {
    /// The password, verified against the acting identity's factor.
    pub password: String,
}

/// Sign in as somebody else — impersonation.
///
/// A *new session*, for an active identity of the target, with a thirty-minute
/// life and the operator recorded on the row. The operator's own session is
/// untouched and is what they return to. The secret comes back in the reply,
/// once, the way an invitation's does: it is not this browser's session, it is
/// a second one the operator is about to put on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInAs {
    /// Who to become.
    pub person: PublicId,
    /// Why. Mandatory, and it lands on the session row as well as the audit
    /// row: the person it was done to is entitled to read it.
    pub reason: String,
}

/// Take the hat off: delete the impersonated session.
///
/// The operator's own cookie is untouched, so this is not a sign-out. Run from
/// the impersonated session, which is the one that is ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EndImpersonation {}

/// Retire a signing key: `retiring` → `retired`, and out of the JWKS.
///
/// Refused while tokens signed under that kid are still alive, because
/// retiring it is what makes them unverifiable. `force` says the operator
/// meant it anyway, which is a decision and therefore lands in the audit with
/// its reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetireKey {
    /// The key, by the `kid` a JWT header names and the JWKS publishes.
    pub kid: String,
    /// Retire it even though tokens signed under it are still alive.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force: bool,
    /// Why.
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{id, round_trip};

    #[test]
    fn platform_commands_round_trip() {
        round_trip(&GrantOperator {
            person: id("p_"),
            reason: "second pair of hands for the cutover".into(),
        });
        round_trip(&RevokeOperator {
            person: id("p_"),
            reason: "the cutover is done".into(),
        });
        round_trip(&ReAuthenticate {
            password: "an obviously fake test password".into(),
        });
        round_trip(&SignInAs {
            person: id("p_"),
            reason: "reproducing the bug they reported".into(),
        });
        round_trip(&EndImpersonation {});
        round_trip(&RetireKey {
            kid: "2026-08-27-a".into(),
            force: false,
            reason: "nothing signed under it is alive".into(),
        });
        round_trip(&RetireKey {
            kid: "2026-08-27-a".into(),
            force: true,
            reason: "the key is believed compromised".into(),
        });
    }

    #[test]
    fn a_retire_that_is_not_forced_says_nothing_about_forcing() {
        let json = serde_json::to_value(RetireKey {
            kid: "k".into(),
            force: false,
            reason: "why".into(),
        })
        .expect("serialises");
        assert!(
            json.get("force").is_none(),
            "the ordinary retire is the quiet one"
        );
    }
}
