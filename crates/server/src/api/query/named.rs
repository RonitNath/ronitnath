//! The query vocabulary: which names this build serves, and what a
//! subscription has to re-read when the world moves.
//!
//! Kept apart from the dispatcher in [`super`] because it is a different kind
//! of thing: `mod.rs` says how a query is run, and this says what the queries
//! *are*. A leg that adds a page adds its name here, its handler beside it,
//! and one arm to [`Named::touched`] — the single question the socket asks.
//!
//! Three tiers share the enum because they share the socket, and one function
//! answers for all of them. What differs is only *how* a tier can answer:
//!
//! * a query whose rows are the acting identity's own moves when that
//!   identity's events do, and the event names the row — one key
//!   ([`Named::changed`]);
//! * a query whose rows are a set the reader shares with other people moves on
//!   events about strangers, and no key off the event is one the reader
//!   provably holds — so the set is re-read whole and diffed against what this
//!   connection last sent ([`Named::watches`], [`Touch::Set`]);
//! * an organization's queries are the same shape but scoped by the
//!   subscription's own `org` parameter, so they can decide per tenant before
//!   reading anything (`super::org::touched`);
//! * the platform's are the whole deployment, so "whose event is it" narrows
//!   nothing and every list names its keys (`super::platform`).

use rn_kernel::ids::IdKey;
use rn_kernel::{Committed, Event, Principal};

use super::{Params, Platform, Touch, org};

/// The queries this build serves.
///
/// A closed set, not a registry: a name that is not here is refused rather
/// than guessed at, which is what keeps `/api/q/<name>` from being a path
/// traversal with extra steps.
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
    /// One organization: who owns it, when it was founded, and its counts.
    Org,
    /// The memberships of an organization, or of one of its groups.
    OrgMembers,
    /// The groups an organization owns.
    OrgGroups,
    /// Whose contact details are visible inside one of its groups.
    OrgContacts,
    /// The documents an organization owns, and the grants on each.
    OrgDocuments,
    /// The invitation links minted into its containers.
    OrgInvitations,
    /// Its audit tail, forward-paginated by offset.
    OrgAudit,
    /// A `/platform` query: the deployment, for a platform operator.
    Platform(Platform),
}

/// How a subscription keeps a query up to date.
///
/// Four shapes, one per kind of result set — see this module's own docs. The
/// fifth, [`Scope::Once`], is not a subscription at all: it takes a row, and a
/// subscription names none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Keys named by the event, narrowed to the acting identity.
    Identity,
    /// A set the reader shares with other people: re-read whole and diffed.
    Set,
    /// An organization's, scoped by the subscription's `org` parameter.
    Org,
    /// The whole deployment, guarded by the operator relation on every read.
    Platform,
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
    Named::Org,
    Named::OrgMembers,
    Named::OrgGroups,
    Named::OrgContacts,
    Named::OrgDocuments,
    Named::OrgInvitations,
    Named::OrgAudit,
    Named::Platform(Platform::Parties),
    Named::Platform(Platform::Party),
    Named::Platform(Platform::Identities),
    Named::Platform(Platform::Identity),
    Named::Platform(Platform::Sessions),
    Named::Platform(Platform::Audit),
    Named::Platform(Platform::Matches),
    Named::Platform(Platform::Match),
    Named::Platform(Platform::Resources),
    Named::Platform(Platform::Resource),
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
            Self::Org => "org",
            Self::OrgMembers => "org-members",
            Self::OrgGroups => "org-groups",
            Self::OrgContacts => "org-contacts",
            Self::OrgDocuments => "org-documents",
            Self::OrgInvitations => "org-invitations",
            Self::OrgAudit => "org-audit",
            Self::Platform(platform) => platform.as_str(),
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
            Self::Org
            | Self::OrgMembers
            | Self::OrgGroups
            | Self::OrgContacts
            | Self::OrgDocuments
            | Self::OrgInvitations
            | Self::OrgAudit => Scope::Org,
            Self::Platform(_) => Scope::Platform,
        }
    }

    /// Whether this query is scoped to an organization rather than to the
    /// acting identity — which is what decides whether it needs the `org`
    /// parameter and the membership re-read behind it.
    #[must_use]
    pub const fn is_org(self) -> bool {
        matches!(self.scope(), Scope::Org)
    }

    /// Read a name, refusing anything not served.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        ALL.iter().copied().find(|q| q.as_str() == raw)
    }

    /// What an event did to this query's result set, for the socket.
    ///
    /// The one entry point, for all three tiers. Everything a caller needs to
    /// answer is here: the committed event (the platform's audit tail is keyed
    /// by the offset it landed at), the principal, the id key, and the
    /// parameters *this subscription* was opened with — because two sockets on
    /// `org-members` for two organizations are two different result sets.
    #[must_use]
    pub fn touched(
        self,
        committed: &Committed,
        principal: &Principal,
        key: &IdKey,
        params: &Params,
    ) -> Touch {
        match self.scope() {
            Scope::Org => org::touched(self, &committed.event, key, params),
            Scope::Set => {
                if self.watches(&committed.event) {
                    Touch::Set
                } else {
                    Touch::None
                }
            }
            Scope::Once => Touch::None,
            Scope::Identity | Scope::Platform => match self.changed(committed, principal, key) {
                keys if keys.is_empty() => Touch::None,
                keys => Touch::Keys(keys),
            },
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
            Self::Invitations => matches!(
                event,
                Event::Invited { .. } | Event::LinkRevoked { .. } | Event::LinkClaimed { .. }
            ),
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
            // Every other scope answers through `touched`'s own arm.
            _ => false,
        }
    }

    /// Which keys an event could have moved, for this principal.
    ///
    /// An empty list means the query is untouched and the socket sends
    /// nothing. It is deliberately allowed to name a key that turns out not to
    /// have changed — a re-read that finds the same row emits no op — but it
    /// must never *miss* one, which is why the arms match on the event's
    /// subject rather than on its name alone.
    #[must_use]
    pub fn changed(self, committed: &Committed, principal: &Principal, key: &IdKey) -> Vec<String> {
        // The platform queries are the deployment's, not the caller's, so the
        // "is this event mine" test below does not apply to them.
        if let Self::Platform(platform) = self {
            return platform.changed(committed, key);
        }
        let event = &committed.event;
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
    use crate::api::query::platform;
    use rn_kernel::ids::Id;

    fn key() -> IdKey {
        IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key")
    }

    fn committed(event: Event) -> Committed {
        Committed { offset: 12, event }
    }

    #[test]
    fn a_name_the_build_does_not_serve_is_refused_rather_than_guessed() {
        assert_eq!(Named::parse("sessions"), Some(Named::Sessions));
        assert_eq!(Named::parse("audit"), Some(Named::Audit));
        assert_eq!(Named::parse("documents"), Some(Named::Documents));
        assert_eq!(Named::parse("group-members"), Some(Named::GroupMembers));
        assert_eq!(Named::parse("org-members"), Some(Named::OrgMembers));
        assert_eq!(
            Named::parse("platform-parties"),
            Some(Named::Platform(Platform::Parties))
        );
        // The shapes that are not names.
        assert_eq!(Named::parse("people"), None);
        assert_eq!(Named::parse("cluster"), None);
        assert_eq!(Named::parse(""), None);
        assert_eq!(Named::parse("../whoami"), None);
    }

    #[test]
    fn every_name_is_served_once_and_spells_itself_the_same_way_twice() {
        let mut names: Vec<&str> = ALL.iter().map(|query| query.as_str()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "two queries share a name");
        for query in ALL {
            assert_eq!(Named::parse(query.as_str()), Some(*query));
        }
    }

    #[test]
    fn an_event_about_somebody_else_moves_nothing() {
        let mine = Principal::Member {
            identity: Id::new(1),
            person: Some(Id::new(2)),
            acting_as: Id::new(2),
            session: Id::new(3),
        };
        let theirs = committed(Event::SignedIn {
            identity: Id::new(99),
            session: Id::new(98),
        });
        assert!(Named::Sessions.changed(&theirs, &mine, &key()).is_empty());
        assert!(Named::Identities.changed(&theirs, &mine, &key()).is_empty());

        let ours = committed(Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(4),
        });
        assert_eq!(Named::Sessions.changed(&ours, &mine, &key()).len(), 1);
        assert!(Named::Identities.changed(&ours, &mine, &key()).is_empty());
    }

    #[test]
    fn an_anonymous_principal_has_no_rows_of_its_own_to_move() {
        let event = committed(Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(2),
        });
        for query in ALL {
            if query.scope() == Scope::Platform {
                continue;
            }
            assert!(
                query
                    .changed(&event, &Principal::Anonymous, &key())
                    .is_empty(),
                "{}",
                query.as_str()
            );
        }
    }

    #[test]
    fn a_platform_query_is_guarded_on_the_read_and_not_in_the_diff() {
        // `changed` names keys for anybody, because it cannot ask the database
        // whether this principal holds the operator relation. `read` can, and
        // does, on every request — which is what `platform::guard` is and what
        // `platform_queries_decline_everyone_but_an_operator` proves over HTTP.
        let event = committed(Event::Registered {
            identity: Id::new(1),
            person: Id::new(2),
            session: Id::new(3),
        });
        let named = Named::Platform(Platform::Parties);
        assert_eq!(
            named.changed(&event, &Principal::Anonymous, &key()).len(),
            1
        );
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

    #[test]
    fn one_entry_point_answers_for_every_tier() {
        let mine = Principal::Member {
            identity: Id::new(1),
            person: Some(Id::new(2)),
            acting_as: Id::new(2),
            session: Id::new(3),
        };
        let claimed = committed(Event::LinkClaimed {
            container: Id::new(1),
            identity: Id::new(1),
            party: Id::new(2),
        });
        let params = Params::default();
        // A shared set answers in whole sets.
        assert_eq!(
            Named::GroupMembers.touched(&claimed, &mine, &key(), &params),
            Touch::Set
        );
        // An identity-scoped one in keys.
        let signed_in = committed(Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(4),
        });
        assert!(matches!(
            Named::Sessions.touched(&signed_in, &mine, &key(), &params),
            Touch::Keys(keys) if keys.len() == 1
        ));
        // A tenant query with no organization named looks at nothing.
        assert_eq!(
            Named::OrgMembers.touched(&claimed, &mine, &key(), &params),
            Touch::None
        );
        // And the deployment's audit tail is keyed by the offset it landed at.
        assert_eq!(
            Named::Platform(Platform::Audit).touched(&signed_in, &mine, &key(), &params),
            Touch::Keys(vec![platform::offset_key(12)])
        );
    }
}
