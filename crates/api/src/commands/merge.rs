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
