//! Which keys an event moved, one arm per deployment-wide list.
//!
//! Split out of [`super::platform`] because it is a different kind of thing:
//! that module says what the platform queries *are* and who may read them, and
//! this says what a committed event did to the rows of each one. Every match
//! here is exhaustive over [`Event`] on purpose — an event added to the kernel
//! forces a decision in each of these lists rather than silently moving no
//! rows, which is the failure mode a wildcard arm would have made invisible.

use rn_kernel::Event;
use rn_kernel::ids::{Id, IdKey, MatchCandidate, Resource};

pub(super) fn parties_changed(event: &Event, key: &IdKey) -> Vec<String> {
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

pub(super) fn identities_changed(event: &Event, key: &IdKey) -> Vec<String> {
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

pub(super) fn sessions_changed(event: &Event, key: &IdKey) -> Vec<String> {
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

pub(super) fn matches_changed(event: &Event, key: &IdKey) -> Vec<String> {
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

pub(super) fn resources_changed(event: &Event, key: &IdKey) -> Vec<String> {
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
