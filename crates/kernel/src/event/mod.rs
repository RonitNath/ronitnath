//! What a command produces, and what the change feed carries.
//!
//! An event is not written by Rust. It is written by the `json_object(…)` in
//! the command's own audit statement, inside the command's transaction, with
//! the rowids of the rows that statement's neighbours just inserted — which is
//! the only way to name a row that does not exist until the batch commits.
//! Rust reads it back: [`Event::from_payload`] is the single direction, used
//! identically by a fresh commit and by an idempotent replay, so the two
//! cannot answer differently.
//!
//! Ids here are internal. An event stays inside the process until the server
//! renders it, at which point the ids are derived
//! ([`Id::public`](crate::ids::Id::public)) like every other id on a wire.

//!
//! The module is three files along its own seams: the variants and the two
//! projections a router and a cache read off them live here, the direction
//! that reads a payload back lives in [`payload`], and the tests that hold
//! both to the command vocabulary live beside them.

mod payload;
#[cfg(test)]
mod tests;

use crate::Offset;
use crate::domain::FactorKind;
use crate::ids::{
    Factor, Group, Id, Identity, Link, MatchCandidate, OidcClient, Organization, Person, Resource,
    Session,
};
use crate::merge::LinkMethod;
use crate::relation::Relation;

/// One thing that happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A person, an identity and its first factors and session, all at once.
    Registered {
        /// The new registration.
        identity: Id<Identity>,
        /// The person it was created for.
        person: Id<Person>,
        /// The session it was given.
        session: Id<Session>,
    },
    /// A session was minted for an existing identity.
    SignedIn {
        /// Whose.
        identity: Id<Identity>,
        /// The new session.
        session: Id<Session>,
    },
    /// A session ended itself.
    SignedOut {
        /// Whose.
        identity: Id<Identity>,
        /// The session that ended.
        session: Id<Session>,
        /// The relying parties that held a token under it and registered a
        /// back-channel endpoint.
        ///
        /// Carried on the event rather than looked up afterwards, because the
        /// batch that ends a session takes its tokens with it: by the time a
        /// consumer sees this row there is nothing left to ask. It is what
        /// lets the logout POSTs happen off the request path, on any node that
        /// reads the feed.
        clients: Vec<Id<OidcClient>>,
    },
    /// Another session of the same identity was ended.
    SessionRevoked {
        /// Whose.
        identity: Id<Identity>,
        /// The session that was ended.
        session: Id<Session>,
        /// The relying parties to tell — see [`Self::SignedOut`].
        clients: Vec<Id<OidcClient>>,
    },
    /// A session ended because a relying party asked it to.
    ///
    /// The same cascade as [`Self::SignedOut`] and a different word, because
    /// the audit row's job is to say what happened: somebody pressed "sign
    /// out" *at another site*, and this deployment obliged.
    SessionEnded {
        /// Whose.
        identity: Id<Identity>,
        /// The session that ended.
        session: Id<Session>,
        /// The relying parties to tell.
        clients: Vec<Id<OidcClient>>,
    },
    /// A session changed the party it speaks as.
    ActingAs {
        /// Whose session.
        identity: Id<Identity>,
        /// The session that moved.
        session: Id<Session>,
        /// The party it now speaks as.
        party: Id<Person>,
    },
    /// A factor was added to an identity.
    FactorAdded {
        /// Whose.
        identity: Id<Identity>,
        /// The new factor.
        factor: Id<Factor>,
        /// What it proves.
        kind: FactorKind,
    },
    /// A factor was removed.
    FactorRemoved {
        /// Whose.
        identity: Id<Identity>,
        /// The factor that is gone.
        factor: Id<Factor>,
    },
    /// An email factor was proven.
    EmailVerified {
        /// Whose.
        identity: Id<Identity>,
        /// The factor that is now verified.
        factor: Id<Factor>,
    },
    /// A party stopped being usable.
    PartyDisabled {
        /// Which.
        party: Id<Person>,
        /// The relying parties that held a token for that party. Every
        /// session of every registration went with the disable, so they are
        /// told the same way a sign-out tells them.
        clients: Vec<Id<OidcClient>>,
    },
    /// A party became usable again.
    PartyEnabled {
        /// Which.
        party: Id<Person>,
    },
    /// Two identities were put on the match queue as possibly one human.
    MatchProposed {
        /// The queued pair.
        candidate: Id<MatchCandidate>,
    },
    /// An operator ruled that a queued pair is two different humans.
    MatchRejected {
        /// The pair that is now closed.
        candidate: Id<MatchCandidate>,
    },
    /// One person absorbed another. The report's `person.merged`; it is named
    /// for the command that produced it because a feed consumer and a router
    /// share one vocabulary, and `method` says which of the two it was.
    PersonMerged {
        /// The person that remains. Always the older of the two.
        survivor: Id<Person>,
        /// The person that is now an alias of the survivor.
        absorbed: Id<Person>,
        /// What proved it.
        method: LinkMethod,
        /// The candidate it came off, if it came off one.
        candidate: Option<Id<MatchCandidate>>,
    },
    /// An identity that had no person of its own joined one. Not a merge:
    /// nothing was absorbed, so no id changed meaning.
    IdentityLinked {
        /// The identity that was attached.
        identity: Id<Identity>,
        /// The person it joined.
        person: Id<Person>,
        /// What proved it.
        method: LinkMethod,
    },
    /// An identity was detached onto a person of its own.
    PersonSplit {
        /// The identity that moved.
        identity: Id<Identity>,
        /// The person it moved to.
        person: Id<Person>,
    },
    /// An organization was founded.
    OrganizationCreated {
        /// Its party row — what memberships and relations address.
        organization: Id<Organization>,
        /// Its resource row — what ownership and `Transfer` address.
        resource: Id<Resource>,
        /// The person who founded it, and its first owner.
        owner: Id<Person>,
    },
    /// A group was created, in an organization or for one person.
    GroupCreated {
        /// Its party row.
        group: Id<Group>,
        /// Its resource row.
        resource: Id<Resource>,
        /// The person or organization that owns it.
        owner: Id<Person>,
    },
    /// An invitation link was minted.
    Invited {
        /// The container it joins.
        container: Id<Group>,
        /// The link that was minted. Its token is in the reply and nowhere
        /// else.
        link: Id<Link>,
    },
    /// An invitation was withdrawn before anybody claimed it.
    LinkRevoked {
        /// The container it would have joined.
        container: Id<Group>,
        /// The link that is gone.
        link: Id<Link>,
    },
    /// An invitation was claimed, and a membership exists that did not.
    LinkClaimed {
        /// The container joined.
        container: Id<Group>,
        /// The registration that claimed it.
        identity: Id<Identity>,
        /// The party that is now a member.
        party: Id<Person>,
    },
    /// A member's role changed.
    RoleSet {
        /// The container.
        container: Id<Group>,
        /// Whose role.
        party: Id<Person>,
    },
    /// A member was removed by somebody who administers the container.
    MemberRemoved {
        /// The container.
        container: Id<Group>,
        /// Who is no longer in it.
        party: Id<Person>,
    },
    /// A member left.
    Left {
        /// The container.
        container: Id<Group>,
        /// Who left.
        party: Id<Person>,
    },
    /// A relation was granted. The object is a kind and a row rather than a
    /// typed id, because a share is over anything the vocabulary registers —
    /// a document by its resource row, a person by their party row.
    Shared {
        /// The registered kind.
        object_kind: String,
        /// The row it addresses.
        object_id: i64,
        /// What was granted.
        relation: Relation,
    },
    /// A relation was withdrawn.
    Revoked {
        /// The registered kind.
        object_kind: String,
        /// The row it addresses.
        object_id: i64,
        /// What was withdrawn.
        relation: Relation,
    },
    /// A resource changed hands. The only event that reports an owner change.
    Transferred {
        /// The resource.
        resource: Id<Resource>,
        /// Its new owner.
        to: Id<Person>,
    },
    /// A document was created, in draft.
    DocumentCreated {
        /// Its resource row.
        document: Id<Resource>,
        /// Who owns it.
        owner: Id<Person>,
    },
    /// A document's draft moved on.
    DocumentEdited {
        /// The document.
        document: Id<Resource>,
        /// The revision the edit produced.
        rev: i64,
    },
    /// A document's draft became its published version.
    DocumentPublished {
        /// The document.
        document: Id<Resource>,
        /// The revision that is now published.
        rev: i64,
    },
    /// A person chose or changed their handle.
    HandleSet {
        /// Whose.
        person: Id<Person>,
    },
    /// A relying party was registered.
    ClientRegistered {
        /// The client.
        client: Id<OidcClient>,
        /// The party that owns it, and whose sector its subjects are under.
        /// `None` is the deployment itself — `platform:*`, which has no
        /// `party` row.
        owner: Option<Id<Person>>,
    },
    /// A relying party's metadata was replaced.
    ClientUpdated {
        /// The client.
        client: Id<OidcClient>,
    },
    /// A relying party got a new secret, and its old one stopped working.
    ClientSecretRotated {
        /// The client.
        client: Id<OidcClient>,
    },
    /// A relying party was withdrawn, with its tokens and consents.
    ClientDeleted {
        /// The client.
        client: Id<OidcClient>,
    },
    /// A new signing key became active, and the previous one began retiring.
    SigningKeyRotated {
        /// The `kid` a JWT header will now name.
        kid: String,
    },
    /// A person consented, and an authorization code was minted.
    Authorized {
        /// The client they consented to.
        client: Id<OidcClient>,
        /// Who consented.
        person: Id<Person>,
    },
    /// A code became a token pair.
    CodeExchanged {
        /// The client that redeemed it.
        client: Id<OidcClient>,
    },
    /// A refresh token was rotated.
    TokenRefreshed {
        /// The client.
        client: Id<OidcClient>,
    },
    /// A client minted a token for its own service principal.
    ServiceTokenIssued {
        /// The client.
        client: Id<OidcClient>,
        /// The service party the token speaks as.
        service: Id<Person>,
    },
    /// A token was revoked at a client's request.
    TokenRevoked {
        /// The client.
        client: Id<OidcClient>,
    },
    /// A person withdrew a consent, and every token it produced with it.
    ConsentRevoked {
        /// The client.
        client: Id<OidcClient>,
        /// Who withdrew it.
        person: Id<Person>,
    },
}

impl Event {
    /// The tag the audit payload carries, which is also the command's route
    /// name — one vocabulary, so a feed consumer and a router agree.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Registered { .. } => "register",
            Self::SignedIn { .. } => "sign-in",
            Self::SignedOut { .. } => "sign-out",
            Self::SessionRevoked { .. } => "revoke-session",
            Self::ActingAs { .. } => "act-as",
            Self::FactorAdded { .. } => "add-factor",
            Self::FactorRemoved { .. } => "remove-factor",
            Self::EmailVerified { .. } => "verify-email",
            Self::PartyDisabled { .. } => "disable",
            Self::PartyEnabled { .. } => "enable",
            Self::MatchProposed { .. } => "propose-match",
            Self::MatchRejected { .. } => "rule-match",
            Self::IdentityLinked { method, .. } | Self::PersonMerged { method, .. } => match method
            {
                LinkMethod::Operator => "rule-match",
                LinkMethod::SelfLink | LinkMethod::Factor | LinkMethod::Import => "confirm-match",
            },
            Self::PersonSplit { .. } => "split",
            Self::OrganizationCreated { .. } => "create-organization",
            Self::GroupCreated { .. } => "create-group",
            Self::Invited { .. } => "invite",
            Self::LinkRevoked { .. } => "revoke-link",
            Self::LinkClaimed { .. } => "claim-link",
            Self::RoleSet { .. } => "set-role",
            Self::MemberRemoved { .. } => "remove-member",
            Self::Left { .. } => "leave",
            Self::Shared { .. } => "share",
            Self::Revoked { .. } => "revoke",
            Self::Transferred { .. } => "transfer",
            Self::DocumentCreated { .. } => "create-document",
            Self::DocumentEdited { .. } => "edit-document",
            Self::DocumentPublished { .. } => "publish-document",
            Self::HandleSet { .. } => "set-handle",
            Self::ClientRegistered { .. } => "register-client",
            Self::ClientUpdated { .. } => "update-client",
            Self::ClientSecretRotated { .. } => "rotate-client-secret",
            Self::ClientDeleted { .. } => "delete-client",
            Self::SigningKeyRotated { .. } => "rotate-signing-key",
            Self::Authorized { .. } => "authorize",
            Self::CodeExchanged { .. } => "exchange-code",
            Self::TokenRefreshed { .. } => "refresh-token",
            Self::ServiceTokenIssued { .. } => "client-credentials",
            Self::TokenRevoked { .. } => "revoke-token",
            Self::ConsentRevoked { .. } => "revoke-consent",
            Self::SessionEnded { .. } => "end-session",
        }
    }

    /// The session this event ended, and the relying parties that have to be
    /// told — the one projection the back-channel logout lane reads.
    ///
    /// `PartyDisabled` is absent on purpose: it ends every session of every
    /// registration at once, so there is no single `sid` to name, and its
    /// logout tokens carry `sub` alone. [`Self::disabled_party`] is its half.
    #[must_use]
    pub fn ended_session(&self) -> Option<(Id<Session>, &[Id<OidcClient>])> {
        match self {
            Self::SignedOut {
                session, clients, ..
            }
            | Self::SessionRevoked {
                session, clients, ..
            }
            | Self::SessionEnded {
                session, clients, ..
            } => Some((*session, clients)),
            _ => None,
        }
    }

    /// The party a disable took every session of, and who to tell.
    #[must_use]
    pub fn disabled_party(&self) -> Option<(Id<Person>, &[Id<OidcClient>])> {
        match self {
            Self::PartyDisabled { party, clients } => Some((*party, clients)),
            _ => None,
        }
    }

    /// The identity this event is about, for cache invalidation.
    pub const fn touches_identity(&self) -> Option<Id<Identity>> {
        match self {
            Self::Registered { identity, .. }
            | Self::SignedIn { identity, .. }
            | Self::SignedOut { identity, .. }
            | Self::SessionRevoked { identity, .. }
            | Self::ActingAs { identity, .. }
            | Self::FactorAdded { identity, .. }
            | Self::FactorRemoved { identity, .. }
            | Self::EmailVerified { identity, .. }
            | Self::PersonSplit { identity, .. }
            | Self::IdentityLinked { identity, .. }
            | Self::LinkClaimed { identity, .. }
            | Self::SessionEnded { identity, .. } => Some(*identity),
            _ => None,
        }
    }

    /// The person this event is about, for cache invalidation.
    pub const fn touches_person(&self) -> Option<Id<Person>> {
        match self {
            Self::Registered { person, .. } => Some(*person),
            Self::PartyDisabled { party, .. } | Self::PartyEnabled { party } => Some(*party),
            Self::ActingAs { party, .. } => Some(*party),
            Self::PersonMerged { survivor, .. } => Some(*survivor),
            Self::PersonSplit { person, .. } | Self::IdentityLinked { person, .. } => Some(*person),
            Self::OrganizationCreated { owner, .. }
            | Self::GroupCreated { owner, .. }
            | Self::DocumentCreated { owner, .. } => Some(*owner),
            Self::LinkClaimed { party, .. }
            | Self::RoleSet { party, .. }
            | Self::MemberRemoved { party, .. }
            | Self::Left { party, .. } => Some(*party),
            Self::Transferred { to, .. } => Some(*to),
            Self::HandleSet { person }
            | Self::Authorized { person, .. }
            | Self::ConsentRevoked { person, .. } => Some(*person),
            _ => None,
        }
    }
}

/// An event and where it sits in the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// The `audit.id` of the row that produced it. A consumer resumes here.
    pub offset: Offset,
    /// What happened.
    pub event: Event,
}
