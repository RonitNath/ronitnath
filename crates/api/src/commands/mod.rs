//! One arguments struct per command in `docs/rebuild/plan.md` §Product
//! contract, grouped by what the command is about.
//!
//! The fields are the minimum the kernel needs to execute the command; nothing
//! the server can determine for itself is on the wire. In particular an
//! identity's `source`, the acting principal, and every `at` timestamp are the
//! server's to stamp, so no command accepts them.

mod identity;
mod merge;
mod org;
mod resource;

pub use identity::{
    ActAs, AddFactor, Disable, Enable, FactorKind, Register, RemoveFactor, RevokeSession, SignIn,
    SignOut, VerifyEmail,
};
pub use merge::{ConfirmMatch, MatchSignal, ProposeMatch, RuleMatch, Split};
pub use org::{
    ClaimLink, CreateGroup, CreateOrganization, Invite, Leave, RemoveMember, RevokeLink, SetRole,
};
pub use resource::{CreateDocument, EditDocument, PublishDocument, Revoke, Share, Transfer};

/// Implement [`Command`](crate::command::Command) for a list of args structs.
macro_rules! command_names {
    ($($ty:ty => $name:literal),+ $(,)?) => {
        $(impl $crate::command::Command for $ty {
            const NAME: &'static str = $name;
        })+

        /// Every command route, in the order `docs/rebuild/plan.md` §Product
        /// contract lists them. The server derives its router from this, the
        /// kernel checks its events against it, and a test asserts nothing
        /// appears twice — so "the vocabulary" is one list rather than three
        /// that agree by inspection.
        pub const ALL_COMMAND_NAMES: &[&str] = &[$($name),+];
    };
}

command_names! {
    Register => "register",
    SignIn => "sign-in",
    SignOut => "sign-out",
    RevokeSession => "revoke-session",
    ActAs => "act-as",
    AddFactor => "add-factor",
    RemoveFactor => "remove-factor",
    VerifyEmail => "verify-email",
    CreateOrganization => "create-organization",
    CreateGroup => "create-group",
    Invite => "invite",
    ClaimLink => "claim-link",
    RevokeLink => "revoke-link",
    SetRole => "set-role",
    RemoveMember => "remove-member",
    Leave => "leave",
    Share => "share",
    Revoke => "revoke",
    Transfer => "transfer",
    CreateDocument => "create-document",
    EditDocument => "edit-document",
    PublishDocument => "publish-document",
    ProposeMatch => "propose-match",
    ConfirmMatch => "confirm-match",
    RuleMatch => "rule-match",
    Split => "split",
    Disable => "disable",
    Enable => "enable",
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_product_contract_is_covered_exactly_once() {
        // The list in docs/rebuild/plan.md §Product contract, verbatim.
        assert_eq!(ALL_COMMAND_NAMES.len(), 28);
        let mut sorted = ALL_COMMAND_NAMES.to_vec();
        sorted.sort_unstable();
        let count = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), count, "two commands share a route name");
    }

    #[test]
    fn route_names_are_lowercase_kebab_case() {
        for name in ALL_COMMAND_NAMES {
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                    && !name.starts_with('-')
                    && !name.ends_with('-'),
                "{name} is not a kebab-case route segment"
            );
        }
    }
}
