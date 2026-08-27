//! The `/platform` queries: the whole deployment, for the one principal that
//! holds `platform:* #operator`.
//!
//! Every other query in this module is "mine" and needs no authorisation
//! beyond having a session — the result set is bounded by the caller's own
//! behaviour. These are the opposite: they are the deployment's parties, its
//! registrations, its sessions, its audit log. So the operator relation is
//! *re-read on every request* ([`guard`]) rather than trusted from the shell
//! that served the bundle or from a cached tier: a relation that was revoked a
//! second ago must stop answering, and the row is the only thing that can say
//! so.
//!
//! ## What a diff can name, and what it cannot
//!
//! [`Platform::changed`] is exhaustive over [`Event`] for every list here, so
//! an event added to the kernel forces a decision rather than silently moving
//! no rows. Two events move rows that no key off the event can name, and they
//! are answered by [`Platform::rereads`] rather than papered over:
//!
//! * [`Event::PersonMerged`] names two *persons*, and the identities they are
//!   made of are not in it. `platform-identities` re-reads its whole set, so
//!   the person column repaints for both sides of the merge.
//! * [`Event::PartyDisabled`] deletes every session the party's identities
//!   held and names none of them, so `platform-sessions` re-reads too.
//!
//! Re-reading a set is strictly narrower than naming keys we cannot justify: a
//! `del` can then only mention a key this connection previously sent
//! (`query::set_ops`). `platform-matches` needs neither, because a merge that
//! came off a candidate carries that candidate's id.

use rn_api::PublicId;
use rn_kernel::cmd::is_platform_operator;
use rn_kernel::error::decline;
use rn_kernel::ids::{self, Id, IdKey, MatchCandidate, Person, Public, Resource};
use rn_kernel::store::Reads;
use rn_kernel::{Committed, Event, Outcome, Principal};

use super::{Params, Row};

/// How many rows a platform list returns by default.
///
/// A bound, not a page size the reader chose: these result sets are the whole
/// deployment, and an operator console that tried to render all of it would
/// be a console that stops rendering. The table in the bundle pages within
/// what arrives.
pub const PAGE: usize = 200;

/// The largest page an operator may ask for.
const MAX: usize = 1_000;

/// The platform queries, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Every party: kind, display, status, created.
    Parties,
    /// One party, with its identities, memberships, owned resources and the
    /// relations it holds as a subject.
    Party,
    /// Every registration, with its factor kinds and the person it resolved to.
    Identities,
    /// One registration, with its sessions and its match candidates.
    Identity,
    /// Every live session on the deployment.
    Sessions,
    /// The whole audit log, newest first.
    Audit,
    /// The match queue.
    Matches,
    /// One candidate, with the `person_link` rows and alias a ruling produced.
    Match,
    /// The resource registry.
    Resources,
    /// One resource, with the relations held on it.
    Resource,
    /// The OpenID client registry.
    OidcClients,
    /// The tokens one session is holding. A drill-in: it takes a `subject`.
    OidcTokens,
}

/// Every platform query this build serves.
pub const ALL: &[Platform] = &[
    Platform::Parties,
    Platform::Party,
    Platform::Identities,
    Platform::Identity,
    Platform::Sessions,
    Platform::Audit,
    Platform::Matches,
    Platform::Match,
    Platform::Resources,
    Platform::Resource,
    Platform::OidcClients,
    Platform::OidcTokens,
];

impl Platform {
    /// The `<name>` in `/api/q/<name>`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parties => "platform-parties",
            Self::Party => "platform-party",
            Self::Identities => "platform-identities",
            Self::Identity => "platform-identity",
            Self::Sessions => "platform-sessions",
            Self::Audit => "platform-audit",
            Self::Matches => "platform-matches",
            Self::Match => "platform-match",
            Self::Resources => "platform-resources",
            Self::Resource => "platform-resource",
            Self::OidcClients => "oidc-clients",
            Self::OidcTokens => "oidc-tokens",
        }
    }

    /// Read a name, refusing anything not served.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        ALL.iter().copied().find(|q| q.as_str() == raw)
    }

    /// Which keys an event could have moved.
    ///
    /// The principal is not consulted: an operator sees the whole deployment,
    /// so "whose event is it" moves nothing here. The guard that decides
    /// whether they see any of it is [`guard`], and it runs on the re-read.
    #[must_use]
    pub fn changed(self, committed: &Committed, key: &IdKey) -> Vec<String> {
        let event = &committed.event;
        match self {
            Self::Parties => parties_changed(event, key),
            Self::Identities => identities_changed(event, key),
            Self::Sessions => sessions_changed(event, key),
            Self::Matches => matches_changed(event, key),
            Self::Resources => resources_changed(event, key),
            // The drill-ins are addressed by a parameter the socket does not
            // carry into `changed`, so they are read rather than followed. A
            // panel that is open while its subject moves is re-read when the
            // reader opens it again.
            Self::Party | Self::Identity | Self::Match | Self::Resource => Vec::new(),
            // The registry moves on events that name a client rather than one
            // of its own keys, and the token list is a drill-in like the
            // others; both answer in sets (`rereads`).
            Self::OidcClients | Self::OidcTokens => Vec::new(),
            // One row per command, keyed by the offset that command landed
            // at — which is the audit row's own id. This is the one query
            // whose key is the position rather than the subject, and the
            // reason `changed` takes the committed event rather than the bare
            // one.
            Self::Audit => vec![offset_key(committed.offset)],
        }
    }

    /// Whether an event moved this query's rows without naming any of them.
    ///
    /// The socket answers `Touch::Set` for these, re-reads the whole result
    /// and diffs it against what it sent. See this module's own docs for the
    /// two cases and why a key cannot cover either.
    #[must_use]
    pub const fn rereads(self, event: &Event) -> bool {
        matches!(
            (self, event),
            (Self::Identities, Event::PersonMerged { .. })
                | (Self::Sessions, Event::PartyDisabled { .. })
                | (
                    Self::OidcClients,
                    Event::ClientRegistered { .. }
                        | Event::ClientUpdated { .. }
                        | Event::ClientSecretRotated { .. }
                        | Event::ClientDeleted { .. }
                )
                | (
                    Self::OidcTokens,
                    Event::CodeExchanged { .. }
                        | Event::TokenRefreshed { .. }
                        | Event::TokenRevoked { .. }
                        | Event::SignedOut { .. }
                        | Event::SessionRevoked { .. }
                        | Event::SessionEnded { .. }
                )
        )
    }
}

/// An audit row's key: zero-padded so a key-ordered map reads chronologically
/// rather than lexicographically wrong at every power of ten.
#[must_use]
pub fn offset_key(offset: u64) -> String {
    format!("{offset:020}")
}

/// The operator relation, re-read.
///
/// Returns the acting person, or declines. Platform administration is a
/// relation and not a column, so this is a query and not a flag — and it is
/// asked once per request, because that is what makes revoking it take effect.
pub async fn guard(reads: &impl Reads, principal: &Principal) -> Outcome<Id<Person>> {
    let Principal::Member {
        person: Some(person),
        ..
    } = principal
    else {
        return decline();
    };
    if is_platform_operator(reads, *person).await? {
        Ok(*person)
    } else {
        decline()
    }
}

/// Run a platform query for a principal that has passed [`guard`].
pub async fn read(
    reads: &impl Reads,
    query: Platform,
    principal: &Principal,
    params: &Params,
) -> Outcome<Vec<Row>> {
    guard(reads, principal).await?;
    match query {
        Platform::Parties => super::platform_parties::list(reads, params).await,
        Platform::Party => super::platform_parties::one(reads, params).await,
        Platform::Identities => super::platform_identities::list(reads, params).await,
        Platform::Identity => super::platform_identities::one(reads, params).await,
        Platform::Sessions => super::platform_sessions::list(reads, params).await,
        Platform::Audit => super::platform_audit::list(reads, params).await,
        Platform::Matches => super::platform_matches::list(reads, params).await,
        Platform::Match => super::platform_matches::one(reads, params).await,
        Platform::Resources => super::platform_resources::list(reads, params).await,
        Platform::Resource => super::platform_resources::one(reads, params).await,
        Platform::OidcClients => super::oidc::clients(reads, params).await,
        Platform::OidcTokens => super::oidc::tokens(reads, params).await,
    }
}

/// The `subject` parameter, decoded to the row a drill-in was opened on.
///
/// A drill-in with no subject reads nothing rather than reading everything:
/// the parameter is the whole question, and a missing one is not a wildcard.
pub fn subject<T: Public>(key: &IdKey, params: &Params) -> Option<Id<T>> {
    let raw = params.subject.as_deref()?;
    let id: PublicId = raw.parse().ok()?;
    ids::decode::<T>(key, &id).ok()
}

/// How many rows a list returns, bounded however it was asked for.
#[must_use]
pub fn page(params: &Params) -> i64 {
    params.limit.unwrap_or(PAGE).clamp(1, MAX) as i64
}

fn parties_changed(event: &Event, key: &IdKey) -> Vec<String> {
    match event {
        Event::Registered { person, .. }
        | Event::PartyDisabled { party: person, .. }
        | Event::PartyEnabled { party: person }
        | Event::PersonSplit { person, .. }
        | Event::IdentityLinked { person, .. } => vec![person.public(key).as_str().to_owned()],
        Event::PersonMerged {
            survivor, absorbed, ..
        } => vec![
            survivor.public(key).as_str().to_owned(),
            absorbed.public(key).as_str().to_owned(),
        ],
        Event::OrganizationCreated { organization, .. } => {
            vec![organization.public(key).as_str().to_owned()]
        }
        Event::GroupCreated { group, .. } => vec![group.public(key).as_str().to_owned()],
        // Nothing else writes a `party` row. Sharing, sessions, factors and
        // documents all leave the party list exactly as it was.
        Event::SignedIn { .. }
        | Event::SignedOut { .. }
        | Event::SessionRevoked { .. }
        | Event::ActingAs { .. }
        | Event::FactorAdded { .. }
        | Event::FactorRemoved { .. }
        | Event::EmailVerified { .. }
        | Event::MatchProposed { .. }
        | Event::MatchRejected { .. }
        | Event::Invited { .. }
        | Event::LinkRevoked { .. }
        | Event::LinkClaimed { .. }
        | Event::RoleSet { .. }
        | Event::MemberRemoved { .. }
        | Event::Left { .. }
        | Event::Shared { .. }
        | Event::Revoked { .. }
        | Event::Transferred { .. }
        | Event::DocumentCreated { .. }
        | Event::DocumentEdited { .. }
        | Event::DocumentPublished { .. }
        // The OpenID Provider writes one `party` row and it is not a person:
        // `register-client` creates the client's own service party, which no
        // list on this tier renders.
        | Event::SessionEnded { .. }
        | Event::HandleSet { .. }
        | Event::ClientRegistered { .. }
        | Event::ClientUpdated { .. }
        | Event::ClientSecretRotated { .. }
        | Event::ClientDeleted { .. }
        | Event::SigningKeyRotated { .. }
        | Event::Authorized { .. }
        | Event::CodeExchanged { .. }
        | Event::TokenRefreshed { .. }
        | Event::ServiceTokenIssued { .. }
        | Event::TokenRevoked { .. }
        // The platform operator's own. Granting or revoking the relation
        // writes a `relation` row and not a `party` one; impersonation writes
        // a session. What changes for a list of parties is nothing.
        | Event::OperatorGranted { .. }
        | Event::OperatorRevoked { .. }
        | Event::ReAuthenticated { .. }
        | Event::Impersonated { .. }
        | Event::ImpersonationEnded { .. }
        | Event::SigningKeyRetired { .. }
        | Event::ConsentRevoked { .. } => Vec::new(),
    }
}

fn identities_changed(event: &Event, key: &IdKey) -> Vec<String> {
    match event {
        Event::Registered { identity, .. }
        | Event::FactorAdded { identity, .. }
        | Event::FactorRemoved { identity, .. }
        | Event::EmailVerified { identity, .. }
        | Event::PersonSplit { identity, .. }
        | Event::IdentityLinked { identity, .. }
        | Event::LinkClaimed { identity, .. } => vec![identity.public(key).as_str().to_owned()],
        // The known miss, written down in this module's own docs: a merge
        // names two persons and no identity, and finding the identities would
        // be a query this function cannot make.
        Event::PersonMerged { .. } => Vec::new(),
        Event::SignedIn { .. }
        | Event::SignedOut { .. }
        | Event::SessionRevoked { .. }
        | Event::ActingAs { .. }
        | Event::PartyDisabled { .. }
        | Event::PartyEnabled { .. }
        | Event::MatchProposed { .. }
        | Event::MatchRejected { .. }
        | Event::OrganizationCreated { .. }
        | Event::GroupCreated { .. }
        | Event::Invited { .. }
        | Event::LinkRevoked { .. }
        | Event::RoleSet { .. }
        | Event::MemberRemoved { .. }
        | Event::Left { .. }
        | Event::Shared { .. }
        | Event::Revoked { .. }
        | Event::Transferred { .. }
        | Event::DocumentCreated { .. }
        | Event::DocumentEdited { .. }
        | Event::DocumentPublished { .. }
        | Event::SessionEnded { .. }
        | Event::HandleSet { .. }
        | Event::ClientRegistered { .. }
        | Event::ClientUpdated { .. }
        | Event::ClientSecretRotated { .. }
        | Event::ClientDeleted { .. }
        | Event::SigningKeyRotated { .. }
        | Event::Authorized { .. }
        | Event::CodeExchanged { .. }
        | Event::TokenRefreshed { .. }
        | Event::ServiceTokenIssued { .. }
        | Event::TokenRevoked { .. }
        // An impersonation is a session of an identity that already existed,
        // and a re-authentication touches a column no identity list renders.
        | Event::OperatorGranted { .. }
        | Event::OperatorRevoked { .. }
        | Event::ReAuthenticated { .. }
        | Event::Impersonated { .. }
        | Event::ImpersonationEnded { .. }
        | Event::SigningKeyRetired { .. }
        | Event::ConsentRevoked { .. } => Vec::new(),
    }
}

fn sessions_changed(event: &Event, key: &IdKey) -> Vec<String> {
    match event {
        Event::Registered { session, .. }
        | Event::SignedIn { session, .. }
        | Event::SignedOut { session, .. }
        | Event::SessionRevoked { session, .. } => {
            vec![session.public(key).as_str().to_owned()]
        }
        // Disabling a party deletes every session its identities hold, and the
        // event names none of them. The rows go; the list learns on re-read.
        _ => Vec::new(),
    }
}

fn matches_changed(event: &Event, key: &IdKey) -> Vec<String> {
    fn one(candidate: Id<MatchCandidate>, key: &IdKey) -> Vec<String> {
        vec![candidate.public(key).as_str().to_owned()]
    }
    match event {
        Event::MatchProposed { candidate } | Event::MatchRejected { candidate } => {
            one(*candidate, key)
        }
        Event::PersonMerged { candidate, .. } => candidate.map(|c| one(c, key)).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn resources_changed(event: &Event, key: &IdKey) -> Vec<String> {
    fn one(resource: Id<Resource>, key: &IdKey) -> Vec<String> {
        vec![resource.public(key).as_str().to_owned()]
    }
    match event {
        Event::OrganizationCreated { resource, .. } | Event::GroupCreated { resource, .. } => {
            one(*resource, key)
        }
        Event::DocumentCreated { document, .. }
        | Event::DocumentEdited { document, .. }
        | Event::DocumentPublished { document, .. } => one(*document, key),
        Event::Transferred { resource, .. } => one(*resource, key),
        _ => Vec::new(),
    }
}

pub use super::platform_statements::statements;

#[cfg(test)]
mod tests {
    use super::*;
    use rn_kernel::ids::Identity;
    use rn_kernel::merge::LinkMethod;

    fn key() -> IdKey {
        IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key")
    }

    fn committed(event: Event) -> Committed {
        Committed { offset: 12, event }
    }

    #[test]
    fn the_audit_tail_is_keyed_by_the_offset_the_command_landed_at() {
        let event = committed(Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(2),
        });
        assert_eq!(
            Platform::Audit.changed(&event, &key()),
            vec!["00000000000000000012".to_owned()]
        );
    }

    #[test]
    fn every_platform_name_is_prefixed_and_unique() {
        let mut names: Vec<&str> = ALL.iter().map(|q| q.as_str()).collect();
        for name in &names {
            // The OpenID Provider's two lists are the exception, and they are
            // named for what they are rather than for who may read them: the
            // registry and the tokens under a session are the *Provider's*
            // vocabulary, and a downstream deployment that mounts it reads
            // them under those names. The tier they are served on is decided
            // by `read`'s operator guard, not by a prefix.
            assert!(
                name.starts_with("platform-") || name.starts_with("oidc-"),
                "{name}"
            );
            assert_eq!(Platform::parse(name), Platform::parse(name));
        }
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "a platform query name is served twice");
        assert_eq!(Platform::parse("sessions"), None);
        assert_eq!(Platform::parse("platform-"), None);
    }

    #[test]
    fn the_two_events_that_name_no_key_are_re_read_whole() {
        let disabled = committed(Event::PartyDisabled {
            party: Id::new(4),
            clients: Vec::new(),
        });
        assert!(Platform::Sessions.rereads(&disabled.event));
        assert!(Platform::Sessions.changed(&disabled, &key()).is_empty());
        assert!(!Platform::Parties.rereads(&disabled.event));

        let merged = committed(Event::PersonMerged {
            survivor: Id::new(1),
            absorbed: Id::new(2),
            method: LinkMethod::Operator,
            candidate: None,
        });
        assert!(Platform::Identities.rereads(&merged.event));
        assert!(!Platform::Matches.rereads(&merged.event));
    }

    #[test]
    fn a_merge_moves_the_two_parties_it_named_and_the_candidate_it_came_off() {
        let key = key();
        let merged = committed(Event::PersonMerged {
            survivor: Id::new(1),
            absorbed: Id::new(2),
            method: LinkMethod::Operator,
            candidate: Some(Id::new(7)),
        });
        assert_eq!(Platform::Parties.changed(&merged, &key).len(), 2);
        assert_eq!(Platform::Matches.changed(&merged, &key).len(), 1);
        // A merge names no identity, so the identities list re-reads instead.
        assert!(Platform::Identities.changed(&merged, &key).is_empty());
        assert!(Platform::Identities.rereads(&merged.event));
    }

    #[test]
    fn a_sign_in_moves_a_session_and_nothing_else() {
        let key = key();
        let event = committed(Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(2),
        });
        assert_eq!(Platform::Sessions.changed(&event, &key).len(), 1);
        assert!(Platform::Parties.changed(&event, &key).is_empty());
        assert!(Platform::Identities.changed(&event, &key).is_empty());
        assert!(Platform::Resources.changed(&event, &key).is_empty());
    }

    #[test]
    fn a_drill_in_with_no_subject_reads_nothing_rather_than_everything() {
        let key = key();
        let params = Params::default();
        assert!(subject::<Person>(&key, &params).is_none());
        let params = Params {
            subject: Some("p_not-an-id".to_owned()),
            ..Params::default()
        };
        assert!(subject::<Person>(&key, &params).is_none());
        // A well-formed id of the wrong kind is refused before any query.
        let real = Id::<Person>::new(3).public(&key);
        let params = Params {
            subject: Some(real.as_str().to_owned()),
            ..Params::default()
        };
        assert!(subject::<Identity>(&key, &params).is_none());
        assert_eq!(subject::<Person>(&key, &params), Some(Id::new(3)));
    }

    #[test]
    fn every_platform_statement_is_planned() {
        let listed = statements();
        let mut names: Vec<&str> = listed.iter().map(|(name, ..)| *name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "a statement is listed twice");
        // One list statement per list query, plus every drill-in's own reads.
        assert_eq!(
            count, 23,
            "a statement was added to a platform query and not to `statements()`"
        );
    }

    #[test]
    fn a_platform_page_is_bounded_however_it_is_asked_for() {
        assert_eq!(page(&Params::default()), PAGE as i64);
        assert_eq!(
            page(&Params {
                limit: Some(0),
                ..Params::default()
            }),
            1
        );
        assert_eq!(
            page(&Params {
                limit: Some(1_000_000),
                ..Params::default()
            }),
            MAX as i64
        );
    }
}
