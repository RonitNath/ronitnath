//! Turning the public ids a command was sent into the rows it acts on.
//!
//! Every refusal here is the uniform decline. A public id that does not
//! decrypt, one of the wrong kind, and one naming a row that does not exist
//! are indistinguishable on purpose: telling them apart is exactly how a
//! caller would learn what exists.

use rn_api::ids::PublicId;

use crate::error::{Outcome, decline};
use crate::ids::{self, Group, Id, IdKey, Identity, Organization, Person, Resource};
use crate::principal::Principal;
use crate::relation::{Subject, SubjectKind};

/// The signed-in person behind a command, and the registration that
/// authenticated. Every command in this leg needs both: one to authorise, one
/// to write on the audit row.
pub(super) fn actor(principal: &Principal) -> Outcome<(Id<Identity>, Id<Person>)> {
    let (identity, acting_as, _) = super::member(principal)?;
    Ok((identity, acting_as))
}

/// A container's party id, from either a group id or an organization id.
///
/// `Invite`, `SetRole` and `Leave` all take a "group", and an organization is
/// its own root group — so both prefixes are accepted and the party id is the
/// same integer either way.
pub(super) fn container(key: &IdKey, id: &PublicId) -> Outcome<Id<Group>> {
    if let Ok(group) = ids::decode::<Group>(key, id) {
        return Ok(group);
    }
    match ids::decode::<Organization>(key, id) {
        Ok(org) => Ok(Id::new(org.get())),
        Err(_) => decline(),
    }
}

/// An organization.
pub(super) fn organization(key: &IdKey, id: &PublicId) -> Outcome<Id<Organization>> {
    ids::decode::<Organization>(key, id).map_or_else(|_| decline(), Ok)
}

/// A resource.
pub(super) fn resource(key: &IdKey, id: &PublicId) -> Outcome<Id<Resource>> {
    ids::decode::<Resource>(key, id).map_or_else(|_| decline(), Ok)
}

/// A subject a relation may be granted to: a person, an organization or a
/// group. `public` and `authenticated` have no public id and so cannot arrive
/// this way at all.
pub(super) fn subject(key: &IdKey, id: &PublicId) -> Outcome<Subject> {
    if let Ok(person) = ids::decode::<Person>(key, id) {
        return Ok(Subject::person(person));
    }
    if let Ok(org) = ids::decode::<Organization>(key, id) {
        return Ok(Subject::organization(org));
    }
    match ids::decode::<Group>(key, id) {
        Ok(group) => Ok(Subject::group(group)),
        Err(_) => decline(),
    }
}

/// A party that may be named as an owner: a person or an organization, and a
/// group never — which is refused here as well as by the schema's trigger,
/// because a caller deserves the same answer whichever layer notices.
pub(super) fn owner(key: &IdKey, id: &PublicId) -> Outcome<(Id<Person>, SubjectKind)> {
    if let Ok(person) = ids::decode::<Person>(key, id) {
        return Ok((person, SubjectKind::Person));
    }
    match ids::decode::<Organization>(key, id) {
        Ok(org) => Ok((Id::new(org.get()), SubjectKind::Organization)),
        Err(_) => decline(),
    }
}
