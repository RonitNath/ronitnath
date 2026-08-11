//! Capabilities: what a membership is allowed to do.
//!
//! A capability is scoped to an `(account_id, identity_id)` membership rather
//! than to the identity alone. The same person can therefore be an operator of
//! one account and a plain member of another without two identities.
//!
//! Capabilities come from two sources, resolved together per request:
//!
//! 1. **The membership's role.** [`MembershipRole::implied`] maps each role to
//!    a bundle in code. Changing a bundle changes what every holder of that
//!    role can do on their next request — no backfill, no per-member rows.
//! 2. **Exception rows** in `membership_capabilities`, for grants that deviate
//!    from the role. These keep a `granted_at`, because a grant that somebody
//!    made by hand is exactly the kind that needs an audit trail.
//!
//! In code a capability is a closed enum, not a string: a guard demanding a
//! capability that does not exist is unrepresentable, and adding a variant
//! forces every `match` — including the role bundles — to say what it means
//! for them. Only the wire form ([`Capability::as_str`]) is stringly, and
//! unknown strings coming back from the database are logged and dropped rather
//! than trusted.
//!
//! The two existing capabilities keep their original names. Future ones should
//! use `resource.action` (`sessions.revoke`, `account.manage`, …) so names
//! still compose once two resources both have a `read`.

use std::collections::BTreeSet;

use crate::auth::model::MembershipRole;

/// Everything a membership can be allowed to do.
///
/// `#[non_exhaustive]` is deliberately absent: exhaustive matches are the
/// mechanism by which a new capability is forced through every role bundle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    /// Implied by every role. Gates `/protected`: proof that a session exists
    /// and carries capabilities, without gating anything that matters.
    TestAuth,
    /// Implied by no role that registration can produce, so `/manage` is
    /// unreachable for a freshly registered account until somebody grants it.
    Manage,
}

impl Capability {
    /// Every capability, for iteration in tests and admin surfaces.
    pub const ALL: &[Self] = &[Self::TestAuth, Self::Manage];

    /// The stored/wire form. Stable: rows in `membership_capabilities` hold
    /// these strings, so renaming a variant must not change its `as_str`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TestAuth => "test-auth",
            Self::Manage => "manage",
        }
    }

    /// Parse a stored string. `None` for anything unknown — the caller decides
    /// whether that is a logged-and-skipped row or an error.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.as_str() == s)
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl MembershipRole {
    /// The capability bundle this role implies.
    ///
    /// This is the primary source of capabilities; `membership_capabilities`
    /// rows are the exceptions layered on top. Registration creates `Owner`
    /// memberships, so `Owner` must not imply [`Capability::Manage`] — the
    /// golden-flow contract is that a fresh registrant cannot reach `/manage`.
    #[must_use]
    pub const fn implied(self) -> &'static [Capability] {
        match self {
            Self::Owner => &[Capability::TestAuth],
            Self::Admin => &[Capability::TestAuth, Capability::Manage],
            Self::Member => &[Capability::TestAuth],
        }
    }
}

/// The capabilities carried by one resolved session: the role bundle unioned
/// with the membership's exception rows.
///
/// Ordered so debug output and log lines are stable across runs, which matters
/// when the test harness diffs them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet(BTreeSet<Capability>);

impl CapabilitySet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Role bundle plus explicit grants, the way [`resolve_session`] builds it.
    ///
    /// Unknown strings among `explicit` are dropped; the caller logs them.
    ///
    /// [`resolve_session`]: crate::auth::store::resolve_session
    #[must_use]
    pub fn resolve<'a>(role: MembershipRole, explicit: impl Iterator<Item = &'a str>) -> Self {
        let mut set: BTreeSet<Capability> = role.implied().iter().copied().collect();
        set.extend(explicit.filter_map(Capability::parse));
        Self(set)
    }

    #[must_use]
    pub fn has(&self, capability: Capability) -> bool {
        self.0.contains(&capability)
    }

    pub fn insert(&mut self, capability: Capability) {
        self.0.insert(capability);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        self.0.iter().copied()
    }

    /// Comma-joined, for a single log field rather than N.
    #[must_use]
    pub fn to_log_string(&self) -> String {
        self.0
            .iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl FromIterator<Capability> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = Capability>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_round_trip() {
        for capability in Capability::ALL {
            assert_eq!(Capability::parse(capability.as_str()), Some(*capability));
        }
        assert_eq!(Capability::parse("no-such-capability"), None);
    }

    #[test]
    fn no_role_registration_can_produce_implies_manage() {
        // Registration creates Owner memberships; /manage's tests depend on a
        // fresh registrant not holding `manage`.
        assert!(
            !MembershipRole::Owner
                .implied()
                .contains(&Capability::Manage)
        );
        assert!(
            MembershipRole::Owner
                .implied()
                .contains(&Capability::TestAuth)
        );
    }

    #[test]
    fn every_role_implies_test_auth() {
        for role in [
            MembershipRole::Owner,
            MembershipRole::Admin,
            MembershipRole::Member,
        ] {
            assert!(role.implied().contains(&Capability::TestAuth));
        }
    }

    #[test]
    fn resolve_unions_role_and_exceptions_and_drops_unknowns() {
        let caps = CapabilitySet::resolve(
            MembershipRole::Owner,
            ["manage", "not-a-real-capability"].into_iter(),
        );
        assert!(caps.has(Capability::TestAuth), "from the role bundle");
        assert!(caps.has(Capability::Manage), "from the exception row");
        assert_eq!(caps.len(), 2, "the unknown string must be dropped");
    }

    #[test]
    fn resolve_with_no_exceptions_is_exactly_the_bundle() {
        let caps = CapabilitySet::resolve(MembershipRole::Owner, std::iter::empty());
        assert!(caps.has(Capability::TestAuth));
        assert!(!caps.has(Capability::Manage));
        assert_eq!(caps.to_log_string(), "test-auth");
    }
}
