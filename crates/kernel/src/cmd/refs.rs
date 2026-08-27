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

/// Who is running a command: the registration that authenticated, the person
/// behind it, and the party the command is *attributed* to.
///
/// The last two are the same until somebody runs `ActAs`. After that they are
/// not, and the difference is the whole of what acting as an organization
/// means here: authority stays with the human — a person's memberships and
/// grants are theirs, and switching a session cannot conjure any — while the
/// audit row records the organization, which is what makes an organization's
/// history readable as its own.
pub(crate) fn actor(principal: &Principal) -> Outcome<(Id<Identity>, Id<Person>, Id<Person>)> {
    let (identity, person, _) = super::member(principal)?;
    Ok((identity, person, acting_as(principal, person)))
}

/// The party a command's audit row is attributed to, for a caller that already
/// holds the person — [`crate::cmd::sign_out`] wants the session too, and the
/// merge commands read the person for their own authorisation first.
///
/// It is the acted-as party, never the person, and never
/// [`super::member`]'s middle value: binding that into `audit.acting_as` is
/// what makes an organization's history read as the human's instead of its
/// own.
pub(crate) fn acting_as(principal: &Principal, person: Id<Person>) -> Id<Person> {
    principal.acting_as().unwrap_or(person)
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
