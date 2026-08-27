//! Merge and split: resolving identities onto a person, and undoing it.
//!
//! Signals propose and proof disposes. Nothing here merges silently, and
//! nothing here is irreversible: `person_link` is append-only and
//! `person_alias` keeps the absorbed person's URLs alive.

use serde::{Deserialize, Serialize};

use crate::ids::PublicId;

/// Why two identities might be the same human. A signal is never sufficient on
/// its own — the kernel refuses a merge that rests on one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchSignal {
    /// The same phone number, verified on both.
    VerifiedPhone,
    /// The same address, verified on both.
    VerifiedEmail,
    /// The same subject at the same OIDC issuer.
    OidcSubject,
    /// One identity claimed a link the other minted.
    ClaimedLink,
    /// The same name, plus a group they both belong to. The weakest signal
    /// there is, and on its own never enough.
    NameAndGroup,
}

/// Put a pair on the match queue, in `proposed`.
///
/// The observation lane raises most of these; the command exists so an
/// operator or a person reviewing their own account can raise one too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposeMatch {
    /// One identity.
    pub identity_a: PublicId,
    /// The other.
    pub identity_b: PublicId,
    /// What suggested them.
    pub signal: MatchSignal,
}

/// A person confirms two of their own identities are theirs, by having signed
/// in to both. The self-link merge: proof the person supplies about themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmMatch {
    /// The queued candidate.
    pub candidate: PublicId,
    /// The session token held on the *other* identity of the pair, when that
    /// is what the caller is proving with. Holding two live sessions at once
    /// is the same evidence as signing in twice.
    ///
    /// Omitted when the proof is a factor both identities have verified. It is
    /// a bearer secret, so it is sent and never returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_session: Option<String>,
    /// The address the *other* identity signs in with, when the proof is its
    /// credentials. A browser has no way to hold two session tokens at once,
    /// so this is the form of "I am both of these" a person can actually give.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_email: Option<String>,
    /// Its password. Verified in-command against the same argon2id hash
    /// `SignIn` reads, and no session is created: proving you can sign in is
    /// not the same as signing in, and this command hands nothing back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_password: Option<String>,
}

/// A platform operator rules on a candidate the person cannot prove
/// themselves. Evidence is mandatory and lands on the `person_link` row, so a
/// ruling is always attributable afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleMatch {
    /// The queued candidate.
    pub candidate: PublicId,
    /// Whether the two are the same human. A rejection closes the candidate
    /// rather than deleting it, so the same pair does not re-queue forever.
    pub same_person: bool,
    /// What the operator relied on.
    pub evidence: String,
}

/// Undo a merge for one identity: detach it onto a fresh person.
///
/// The older person survives, sessions are untouched — they bind the identity
/// — and product rows do not move, because they reference the identity too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Split {
    /// The identity to detach.
    pub identity: PublicId,
    /// Why, recorded on the new `person_link` row.
    pub evidence: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{id, round_trip};

    #[test]
    fn merge_commands_round_trip() {
        round_trip(&ProposeMatch {
            identity_a: id("i_"),
            identity_b: id("i_"),
            signal: MatchSignal::NameAndGroup,
        });
        round_trip(&ConfirmMatch {
            candidate: id("m_"),
            other_session: None,
            other_email: None,
            other_password: None,
        });
        round_trip(&ConfirmMatch {
            candidate: id("m_"),
            other_session: Some("an obviously fake token".into()),
            other_email: None,
            other_password: None,
        });
        round_trip(&ConfirmMatch {
            candidate: id("m_"),
            other_session: None,
            other_email: Some("other@example.invalid".into()),
            other_password: Some("an obviously fake password".into()),
        });
        round_trip(&RuleMatch {
            candidate: id("m_"),
            same_person: true,
            evidence: "matching government id on file".into(),
        });
        round_trip(&Split {
            identity: id("i_"),
            evidence: "merged in error, 2026-08-26".into(),
        });
    }

    #[test]
    fn signals_spell_themselves_out_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&MatchSignal::OidcSubject).unwrap(),
            "\"oidc_subject\""
        );
        round_trip(&MatchSignal::VerifiedPhone);
    }
}
