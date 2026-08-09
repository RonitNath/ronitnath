//! Capabilities: what a membership is allowed to do.
//!
//! A capability is a flat string stored in `membership_capabilities`, scoped to
//! an `(account_id, identity_id)` membership rather than to the identity alone.
//! The same person can therefore be an operator of one account and a plain
//! member of another without two identities.
//!
//! Two capabilities exist so far, and the difference between them is the whole
//! point of the guard tests:
//!
//! - [`TEST_AUTH`] is in [`DEFAULT_GRANTS`], so every registration gets it. It
//!   is what `/protected` requires: proof that a session exists and carries
//!   capabilities, without gating anything that matters.
//! - [`MANAGE`] is deliberately *not* granted by default. Nothing in the
//!   registration path can hand it out, so `/manage` is unreachable for a
//!   freshly registered account — signed in or not.

use std::collections::BTreeSet;

/// Granted to every new membership. Gates `/protected`.
pub const TEST_AUTH: &str = "test-auth";

/// Never granted by default. Gates `/manage`.
pub const MANAGE: &str = "manage";

/// Capabilities written for every membership created by registration.
///
/// Adding a capability here grants it to all *future* registrations only;
/// existing memberships are unaffected, since grants are rows, not a computed
/// view. That is intentional — a grant should be auditable by `granted_at`.
pub const DEFAULT_GRANTS: &[&str] = &[TEST_AUTH];

/// The capabilities carried by one resolved session.
///
/// Ordered so debug output and log lines are stable across runs, which matters
/// when the test harness diffs them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet(BTreeSet<String>);

impl CapabilitySet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn has(&self, capability: &str) -> bool {
        self.0.contains(capability)
    }

    pub fn insert(&mut self, capability: impl Into<String>) {
        self.0.insert(capability.into());
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    /// Comma-joined, for a single log field rather than N.
    #[must_use]
    pub fn to_log_string(&self) -> String {
        self.0
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl FromIterator<String> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_grants_include_test_auth_but_not_manage() {
        assert!(DEFAULT_GRANTS.contains(&TEST_AUTH));
        assert!(
            !DEFAULT_GRANTS.contains(&MANAGE),
            "`manage` must never become a default grant; /manage's test asserts it stays unreachable"
        );
    }

    #[test]
    fn set_reports_membership() {
        let caps: CapabilitySet = DEFAULT_GRANTS.iter().map(|c| (*c).to_string()).collect();
        assert!(caps.has(TEST_AUTH));
        assert!(!caps.has(MANAGE));
        assert_eq!(caps.to_log_string(), "test-auth");
    }
}
