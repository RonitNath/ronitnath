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

use serde_json::Value as Json;

use crate::Offset;
use crate::domain::FactorKind;
use crate::ids::{
    Factor, Group, Id, Identity, Link, Organization, Person, Resource, Session, Table,
};
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
    },
    /// Another session of the same identity was ended.
    SessionRevoked {
        /// Whose.
        identity: Id<Identity>,
        /// The session that was ended.
        session: Id<Session>,
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
    },
    /// A party became usable again.
    PartyEnabled {
        /// Which.
        party: Id<Person>,
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
    /// A member left.
    Left {
        /// The container.
        container: Id<Group>,
        /// Who left.
        party: Id<Person>,
    },
    /// A relation was granted on a resource.
    Shared {
        /// The resource.
        resource: Id<Resource>,
        /// What was granted.
        relation: Relation,
    },
    /// A relation was withdrawn.
    Revoked {
        /// The resource.
        resource: Id<Resource>,
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
            Self::FactorAdded { .. } => "add-factor",
            Self::FactorRemoved { .. } => "remove-factor",
            Self::EmailVerified { .. } => "verify-email",
            Self::PartyDisabled { .. } => "disable",
            Self::PartyEnabled { .. } => "enable",
            Self::OrganizationCreated { .. } => "create-organization",
            Self::GroupCreated { .. } => "create-group",
            Self::Invited { .. } => "invite",
            Self::LinkClaimed { .. } => "claim-link",
            Self::RoleSet { .. } => "set-role",
            Self::Left { .. } => "leave",
            Self::Shared { .. } => "share",
            Self::Revoked { .. } => "revoke",
            Self::Transferred { .. } => "transfer",
            Self::DocumentCreated { .. } => "create-document",
            Self::DocumentEdited { .. } => "edit-document",
            Self::DocumentPublished { .. } => "publish-document",
        }
    }

    /// The identity this event is about, for cache invalidation.
    pub const fn touches_identity(&self) -> Option<Id<Identity>> {
        match self {
            Self::Registered { identity, .. }
            | Self::SignedIn { identity, .. }
            | Self::SignedOut { identity, .. }
            | Self::SessionRevoked { identity, .. }
            | Self::FactorAdded { identity, .. }
            | Self::FactorRemoved { identity, .. }
            | Self::EmailVerified { identity, .. }
            | Self::LinkClaimed { identity, .. } => Some(*identity),
            _ => None,
        }
    }

    /// The person this event is about, for cache invalidation.
    pub const fn touches_person(&self) -> Option<Id<Person>> {
        match self {
            Self::Registered { person, .. } => Some(*person),
            Self::PartyDisabled { party } | Self::PartyEnabled { party } => Some(*party),
            Self::OrganizationCreated { owner, .. }
            | Self::GroupCreated { owner, .. }
            | Self::DocumentCreated { owner, .. } => Some(*owner),
            Self::LinkClaimed { party, .. }
            | Self::RoleSet { party, .. }
            | Self::Left { party, .. } => Some(*party),
            Self::Transferred { to, .. } => Some(*to),
            _ => None,
        }
    }

    /// Read an event out of an audit row's payload.
    ///
    /// A payload that does not parse is not a decline and not a panic: it is a
    /// row this build does not understand, which is what a consumer resuming
    /// across a release upgrade will meet.
    pub fn from_payload(payload: &str) -> Option<Self> {
        let json: Json = serde_json::from_str(payload).ok()?;
        Some(match json.get("event")?.as_str()? {
            "register" => Self::Registered {
                identity: field(&json, "identity")?,
                person: field(&json, "person")?,
                session: field(&json, "session")?,
            },
            "sign-in" => Self::SignedIn {
                identity: field(&json, "identity")?,
                session: field(&json, "session")?,
            },
            "sign-out" => Self::SignedOut {
                identity: field(&json, "identity")?,
                session: field(&json, "session")?,
            },
            "revoke-session" => Self::SessionRevoked {
                identity: field(&json, "identity")?,
                session: field(&json, "session")?,
            },
            "add-factor" => Self::FactorAdded {
                identity: field(&json, "identity")?,
                factor: field(&json, "factor")?,
                kind: <FactorKind as crate::domain::Vocabulary>::parse(
                    json.get("kind")?.as_str()?,
                )?,
            },
            "remove-factor" => Self::FactorRemoved {
                identity: field(&json, "identity")?,
                factor: field(&json, "factor")?,
            },
            "verify-email" => Self::EmailVerified {
                identity: field(&json, "identity")?,
                factor: field(&json, "factor")?,
            },
            "disable" => Self::PartyDisabled {
                party: field(&json, "party")?,
            },
            "enable" => Self::PartyEnabled {
                party: field(&json, "party")?,
            },
            "create-organization" => Self::OrganizationCreated {
                organization: field(&json, "organization")?,
                resource: field(&json, "resource")?,
                owner: field(&json, "owner")?,
            },
            "create-group" => Self::GroupCreated {
                group: field(&json, "group")?,
                resource: field(&json, "resource")?,
                owner: field(&json, "owner")?,
            },
            "invite" => Self::Invited {
                container: field(&json, "container")?,
                link: field(&json, "link")?,
            },
            "claim-link" => Self::LinkClaimed {
                container: field(&json, "container")?,
                identity: field(&json, "identity")?,
                party: field(&json, "party")?,
            },
            "set-role" => Self::RoleSet {
                container: field(&json, "container")?,
                party: field(&json, "party")?,
            },
            "leave" => Self::Left {
                container: field(&json, "container")?,
                party: field(&json, "party")?,
            },
            "share" => Self::Shared {
                resource: field(&json, "resource")?,
                relation: Relation::parse(json.get("relation")?.as_str()?)?,
            },
            "revoke" => Self::Revoked {
                resource: field(&json, "resource")?,
                relation: Relation::parse(json.get("relation")?.as_str()?)?,
            },
            "transfer" => Self::Transferred {
                resource: field(&json, "resource")?,
                to: field(&json, "to")?,
            },
            "create-document" => Self::DocumentCreated {
                document: field(&json, "document")?,
                owner: field(&json, "owner")?,
            },
            "edit-document" => Self::DocumentEdited {
                document: field(&json, "document")?,
                rev: json.get("rev")?.as_i64()?,
            },
            "publish-document" => Self::DocumentPublished {
                document: field(&json, "document")?,
                rev: json.get("rev")?.as_i64()?,
            },
            _ => return None,
        })
    }
}

/// One id out of a payload the database wrote. Generic, because a closure
/// would have to pick one table and every variant names a different one.
fn field<T: Table>(json: &Json, name: &str) -> Option<Id<T>> {
    json.get(name)?.as_i64().map(Id::new)
}

/// An event and where it sits in the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// The `audit.id` of the row that produced it. A consumer resumes here.
    pub offset: Offset,
    /// What happened.
    pub event: Event,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_the_database_wrote_reads_back_as_the_event() {
        let payload = r#"{"event":"register","identity":3,"person":2,"session":9}"#;
        assert_eq!(
            Event::from_payload(payload),
            Some(Event::Registered {
                identity: Id::new(3),
                person: Id::new(2),
                session: Id::new(9),
            })
        );
    }

    #[test]
    fn every_variant_names_the_command_that_produced_it() {
        let events = [
            Event::Registered {
                identity: Id::new(1),
                person: Id::new(1),
                session: Id::new(1),
            },
            Event::SignedIn {
                identity: Id::new(1),
                session: Id::new(1),
            },
            Event::SignedOut {
                identity: Id::new(1),
                session: Id::new(1),
            },
            Event::SessionRevoked {
                identity: Id::new(1),
                session: Id::new(1),
            },
            Event::FactorAdded {
                identity: Id::new(1),
                factor: Id::new(1),
                kind: FactorKind::Email,
            },
            Event::FactorRemoved {
                identity: Id::new(1),
                factor: Id::new(1),
            },
            Event::EmailVerified {
                identity: Id::new(1),
                factor: Id::new(1),
            },
            Event::PartyDisabled { party: Id::new(1) },
            Event::PartyEnabled { party: Id::new(1) },
        ];
        for event in &events {
            let name = event.name();
            assert!(
                rn_api::commands::ALL_COMMAND_NAMES.contains(&name),
                "{name} is not a command route"
            );
        }
        assert_eq!(events.len(), 9, "K1 produces nine events");
    }

    #[test]
    fn a_payload_from_a_build_that_knew_more_is_skipped_rather_than_guessed() {
        assert_eq!(Event::from_payload(r#"{"event":"merge"}"#), None);
        assert_eq!(Event::from_payload(r#"{"event":"register"}"#), None);
        assert_eq!(Event::from_payload("not json"), None);
    }

    #[test]
    fn an_event_says_whose_cached_resolution_it_invalidates() {
        let registered = Event::Registered {
            identity: Id::new(3),
            person: Id::new(2),
            session: Id::new(9),
        };
        assert_eq!(registered.touches_identity(), Some(Id::new(3)));
        assert_eq!(registered.touches_person(), Some(Id::new(2)));

        let disabled = Event::PartyDisabled { party: Id::new(2) };
        assert_eq!(disabled.touches_identity(), None);
        assert_eq!(disabled.touches_person(), Some(Id::new(2)));
    }
}
