//! The query vocabulary: which names this build serves, and what a
//! subscription has to re-read when the world moves.
//!
//! Kept apart from the dispatcher in [`super`] because it is a different kind
//! of thing: `mod.rs` says how a query is run, and this says what the queries
//! *are*. A leg that adds a page adds its name here, its handler beside it,
//! and one arm to each of the two questions a socket asks — which keys an
//! event moved ([`Named::changed`]) or whether it moved the set at all
//! ([`Named::watches`]).

use rn_kernel::ids::IdKey;
use rn_kernel::{Event, Principal};

/// The queries this leg serves.
///
/// A closed set, not a registry: every one of them is `mine`, and a query that
/// took an owner parameter would need an authorisation this enum's shape makes
/// unnecessary. U2–U4 add their own alongside their pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    /// The sessions of the acting identity — the device list.
    Sessions,
    /// The identities resolved onto the acting person, and their factors.
    Identities,
    /// The acting identity's own audit rows, forward-paginated by offset.
    Audit,
    /// The groups the acting person belongs to.
    Groups,
    /// Everyone in those groups.
    GroupMembers,
    /// The invitations the acting identity minted.
    Invitations,
    /// Every document the acting principal can reach.
    Documents,
    /// One document, in full. The only query here that takes a row.
    Document,
    /// Every grant on those documents.
    Shares,
    /// Proposed pairs involving the acting person's registrations.
    Matches,
}

/// How a subscription keeps a query up to date.
///
/// Two shapes, because the member tier has two shapes of result set. A
/// query whose rows all belong to the asking *identity* moves only when that
/// identity's own events do, and the event names the row — so the socket
/// re-reads one key. A query whose rows are a *set* the reader shares with
/// other people — a group's roster, a document somebody else edited — moves
/// on events about strangers, and the event cannot name a key the reader
/// holds without the socket knowing what the reader already has. So the set
/// is re-read whole and diffed against what was last sent, which is the only
/// way a row that *left* the set can be reported at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Keys named by the event ([`Named::changed`]).
    Identity,
    /// The whole result set, re-read and diffed ([`Named::watches`]).
    Set,
    /// Not subscribable: it takes a row, and a subscription names none.
    Once,
}

/// Every query name this build serves, for the route matrix.
pub const ALL: &[Named] = &[
    Named::Sessions,
    Named::Identities,
    Named::Audit,
    Named::Groups,
    Named::GroupMembers,
    Named::Invitations,
    Named::Documents,
    Named::Document,
    Named::Shares,
    Named::Matches,
];

impl Named {
    /// The `<name>` in `/api/q/<name>`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Identities => "identities",
            Self::Audit => "audit",
            Self::Groups => "groups",
            Self::GroupMembers => "group-members",
            Self::Invitations => "invitations",
            Self::Documents => "documents",
            Self::Document => "document",
            Self::Shares => "shares",
            Self::Matches => "matches",
        }
    }

    /// How a subscription keeps this query up to date.
    #[must_use]
    pub const fn scope(self) -> Scope {
        match self {
            Self::Sessions | Self::Identities | Self::Audit => Scope::Identity,
            Self::Groups
            | Self::GroupMembers
            | Self::Invitations
            | Self::Documents
            | Self::Shares
            | Self::Matches => Scope::Set,
            Self::Document => Scope::Once,
        }
    }

    /// Whether an event could have moved a [`Scope::Set`] query's rows.
    ///
    /// Coarse on purpose: it answers about the *kind* of event, not about
    /// whose it was, because whose it was is a question only a query can
    /// answer and the answer is the re-read. Naming an event that turns out to
    /// have changed nothing costs one read and sends nothing; missing one
    /// leaves a reader looking at a stale page, so the arms err towards
    /// reading.
    #[must_use]
    pub const fn watches(self, event: &Event) -> bool {
        match self {
            Self::Groups => matches!(
                event,
                Event::GroupCreated { .. }
                    | Event::LinkClaimed { .. }
                    | Event::RoleSet { .. }
                    | Event::Left { .. }
                    | Event::Transferred { .. }
            ),
            Self::GroupMembers => matches!(
                event,
                Event::GroupCreated { .. }
                    | Event::LinkClaimed { .. }
                    | Event::RoleSet { .. }
                    | Event::Left { .. }
                    | Event::Shared { .. }
                    | Event::Revoked { .. }
                    | Event::PersonMerged { .. }
            ),
            Self::Invitations => matches!(event, Event::Invited { .. } | Event::LinkClaimed { .. }),
            Self::Documents => matches!(
                event,
                Event::DocumentCreated { .. }
                    | Event::DocumentEdited { .. }
                    | Event::DocumentPublished { .. }
                    | Event::Shared { .. }
                    | Event::Revoked { .. }
                    | Event::Transferred { .. }
            ),
            Self::Shares => matches!(
                event,
                Event::Shared { .. }
                    | Event::Revoked { .. }
                    | Event::DocumentCreated { .. }
                    | Event::Transferred { .. }
            ),
            Self::Matches => matches!(
                event,
                Event::MatchProposed { .. }
                    | Event::MatchRejected { .. }
                    | Event::PersonMerged { .. }
                    | Event::IdentityLinked { .. }
                    | Event::PersonSplit { .. }
            ),
            Self::Sessions | Self::Identities | Self::Audit | Self::Document => false,
        }
    }

    /// Read a name, refusing anything not served.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        ALL.iter().copied().find(|q| q.as_str() == raw)
    }

    /// Which keys an event could have moved, for this principal.
    ///
    /// An empty list means the query is untouched and the socket sends
    /// nothing. It is deliberately allowed to name a key that turns out not to
    /// have changed — a re-read that finds the same row emits no op — but it
    /// must never *miss* one, which is why the arms match on the event's
    /// subject rather than on its name alone.
    #[must_use]
    pub fn changed(self, event: &Event, principal: &Principal, key: &IdKey) -> Vec<String> {
        let Some(mine) = principal.identity() else {
            return Vec::new();
        };
        // Only the identity-scoped queries answer here, and only about their
        // own reader: a set-scoped query moves on other people's events, which
        // is precisely what a key named off this event cannot express.
        if self.scope() != Scope::Identity || event.touches_identity() != Some(mine) {
            return Vec::new();
        }
        match (self, event) {
            (Self::Sessions, Event::Registered { session, .. })
            | (Self::Sessions, Event::SignedIn { session, .. })
            | (Self::Sessions, Event::SignedOut { session, .. })
            | (Self::Sessions, Event::SessionRevoked { session, .. }) => {
                vec![session.public(key).as_str().to_owned()]
            }
            (Self::Sessions, _) => Vec::new(),
            // A factor's row is nested inside its identity's, so the identity
            // is the key that moved, not the factor.
            (Self::Identities, Event::Registered { identity, .. })
            | (Self::Identities, Event::FactorAdded { identity, .. })
            | (Self::Identities, Event::FactorRemoved { identity, .. })
            | (Self::Identities, Event::EmailVerified { identity, .. }) => {
                vec![identity.public(key).as_str().to_owned()]
            }
            (Self::Identities, _) => Vec::new(),
            (Self::Audit, _) => Vec::new(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rn_kernel::ids::Id;

    #[test]
    fn a_name_the_build_does_not_serve_is_refused_rather_than_guessed() {
        assert_eq!(Named::parse("sessions"), Some(Named::Sessions));
        assert_eq!(Named::parse("audit"), Some(Named::Audit));
        assert_eq!(Named::parse("documents"), Some(Named::Documents));
        assert_eq!(Named::parse("group-members"), Some(Named::GroupMembers));
        // The tiers that are not this one, and the shapes that are not names.
        assert_eq!(Named::parse("people"), None);
        assert_eq!(Named::parse("cluster"), None);
        assert_eq!(Named::parse(""), None);
        assert_eq!(Named::parse("../whoami"), None);
    }

    #[test]
    fn every_name_is_served_once_and_spells_itself_the_same_way_twice() {
        let mut names: Vec<&str> = ALL.iter().map(|query| query.as_str()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two queries share a name");
        for query in ALL {
            assert_eq!(Named::parse(query.as_str()), Some(*query));
        }
    }

    #[test]
    fn an_event_about_somebody_else_moves_nothing() {
        let key = IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key");
        let mine = Principal::Member {
            identity: Id::new(1),
            person: Some(Id::new(2)),
            acting_as: Id::new(2),
            session: Id::new(3),
        };
        let theirs = Event::SignedIn {
            identity: Id::new(99),
            session: Id::new(98),
        };
        assert!(Named::Sessions.changed(&theirs, &mine, &key).is_empty());
        assert!(Named::Identities.changed(&theirs, &mine, &key).is_empty());

        let ours = Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(4),
        };
        assert_eq!(Named::Sessions.changed(&ours, &mine, &key).len(), 1);
        assert!(Named::Identities.changed(&ours, &mine, &key).is_empty());
    }

    #[test]
    fn an_anonymous_principal_has_no_rows_to_move() {
        let key = IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key");
        let event = Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(2),
        };
        for query in ALL {
            assert!(
                query
                    .changed(&event, &Principal::Anonymous, &key)
                    .is_empty(),
                "{}",
                query.as_str()
            );
        }
    }

    #[test]
    fn a_shared_set_is_watched_and_an_identity_scoped_one_is_not() {
        let claimed = Event::LinkClaimed {
            container: Id::new(1),
            identity: Id::new(2),
            party: Id::new(3),
        };
        assert!(Named::GroupMembers.watches(&claimed));
        assert!(Named::Groups.watches(&claimed));
        assert!(!Named::Documents.watches(&claimed));
        // The identity-scoped queries answer through `changed`, never here.
        for query in [Named::Sessions, Named::Identities, Named::Audit] {
            assert!(!query.watches(&claimed), "{}", query.as_str());
        }
        // And the one that takes a row is not subscribable at all.
        assert_eq!(Named::Document.scope(), Scope::Once);
    }
}
