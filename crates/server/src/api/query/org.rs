//! The `/org` tier's queries: scope, authorisation, and which of them an event
//! could have moved.
//!
//! Every one of these takes an organization's public id and is answered only
//! for a principal who holds `admin` or better *on that organization*, read
//! back from `membership` on this request rather than believed from a token or
//! a previous one ([`scope`]). An operator of A therefore declines everything
//! of B, and the decline is the uniform one — the same bytes as "no such
//! organization", because telling those apart is how a caller enumerates
//! tenants.
//!
//! The second half of a query is [`touched`]: which of its keys a change-feed
//! event could have moved. Some of these sets are keyed by rows whose
//! membership in the set cannot be decided from an event alone — a
//! `GroupCreated` does not say whose organization it landed in — so those
//! answer [`Touch::Set`] and the socket diffs the whole result against what it
//! last sent. That is strictly narrower than naming a key we cannot yet
//! justify: a `del` for a key that was never ours would tell a reader that the
//! id exists.

use rn_api::ids::PublicId;
use rn_kernel::error::{Outcome, decline};
use rn_kernel::ids::{self, Group, Id, IdKey, Organization, Person};
use rn_kernel::org::{self, MemberRole};
use rn_kernel::resource;
use rn_kernel::store::Reads;
use rn_kernel::{Event, Principal};

use super::{Named, Params, Row, org_feed, org_rows};

/// What an event did to a query's result set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Touch {
    /// Nothing. The socket sends no message.
    None,
    /// Exactly these keys; re-read them and send the difference.
    Keys(Vec<String>),
    /// Something in this set moved and the event does not say which row.
    /// Re-read the set and diff it against what was last sent.
    Set,
}

/// One organization, resolved and authorised for this request.
#[derive(Debug, Clone, Copy)]
pub struct Scope {
    /// The organization's id under the organization tag.
    pub org: Id<Organization>,
    /// The same row as a party — what memberships and relations address.
    pub party: Id<Person>,
    /// The person asking.
    pub person: Id<Person>,
    /// What they hold on it.
    pub role: MemberRole,
}

/// Resolve the `org` parameter and prove the caller operates it.
///
/// Three refusals, one answer: no organization named, an id that does not
/// decrypt under the organization tag, and a person who is not an admin of it
/// all decline identically.
pub async fn scope(reads: &impl Reads, principal: &Principal, params: &Params) -> Outcome<Scope> {
    let Principal::Member {
        person: Some(person),
        ..
    } = principal
    else {
        return decline();
    };
    let Some(named) = params.org.as_ref() else {
        return decline();
    };
    let Ok(org) = ids::decode::<Organization>(reads.ids(), named) else {
        return decline();
    };
    let party: Id<Person> = Id::new(org.get());
    let Some(role) = org::role_of(reads, party, *person).await? else {
        return decline();
    };
    if !role.covers(MemberRole::Admin) {
        return decline();
    }
    Ok(Scope {
        org,
        party,
        person: *person,
        role,
    })
}

/// A group of this organization, named by the `group` parameter.
///
/// Ownership is the gate: a group whose `resource.owner_party_id` is not this
/// organization is not this organization's group, whoever asked.
pub async fn group_of(reads: &impl Reads, scope: Scope, named: &PublicId) -> Outcome<Id<Group>> {
    let Ok(group) = ids::decode::<Group>(reads.ids(), named) else {
        return decline();
    };
    match resource::of_group(reads, group).await? {
        Some(row) if row.owner_party_id == scope.party && row.kind == resource::kinds::GROUP => {
            Ok(group)
        }
        _ => decline(),
    }
}

/// Run one of the organization queries.
pub async fn read(
    reads: &impl Reads,
    query: Named,
    principal: &Principal,
    params: &Params,
) -> Outcome<Vec<Row>> {
    let scope = scope(reads, principal, params).await?;
    match query {
        Named::Org => org_rows::overview(reads, scope).await,
        Named::OrgMembers => org_rows::members(reads, scope, params).await,
        Named::OrgGroups => org_rows::groups(reads, scope).await,
        Named::OrgContacts => org_rows::contacts(reads, scope, params).await,
        Named::OrgDocuments => org_feed::documents(reads, scope).await,
        Named::OrgInvitations => org_feed::invitations(reads, scope, params).await,
        Named::OrgAudit => org_feed::audit(reads, scope, params).await,
        // The member and platform tiers' queries never reach here.
        _ => Ok(Vec::new()),
    }
}

/// The party ids of an organization's containers: the organization's own party
/// — it is its own root group — and every group it owns.
pub async fn containers(reads: &impl Reads, scope: Scope) -> Outcome<Vec<i64>> {
    let mut ids = vec![scope.party.get()];
    ids.extend(
        org_rows::group_ids(reads, scope)
            .await?
            .into_iter()
            .map(Id::get),
    );
    Ok(ids)
}

/// What an event did to one organization query's result set.
///
/// The organization is taken from the subscription's own parameters and
/// decoded here, so an event about another tenant's container answers
/// [`Touch::None`] without a database read. Authorisation is still re-read
/// when the rows themselves are: this decides what to *look at*, never what to
/// send.
#[must_use]
pub fn touched(query: Named, event: &Event, key: &IdKey, params: &Params) -> Touch {
    let Some(org) = params
        .org
        .as_ref()
        .and_then(|id| ids::decode::<Organization>(key, id).ok())
    else {
        return Touch::None;
    };
    let party = org.get();
    let mine = |container: Id<Group>| container.get() == party;
    match query {
        Named::Org => overview_touch(event, party, mine),
        Named::OrgMembers => membership_touch(event, params, key, party),
        Named::OrgGroups => match event {
            Event::GroupCreated { .. } | Event::Transferred { .. } => Touch::Set,
            Event::LinkClaimed { .. }
            | Event::RoleSet { .. }
            | Event::MemberRemoved { .. }
            | Event::Left { .. } => Touch::Set,
            _ => Touch::None,
        },
        Named::OrgContacts => match event {
            Event::Shared { object_kind, .. } | Event::Revoked { object_kind, .. }
                if object_kind == "person" =>
            {
                Touch::Set
            }
            _ => Touch::None,
        },
        Named::OrgDocuments => match event {
            Event::DocumentCreated { .. }
            | Event::DocumentEdited { .. }
            | Event::DocumentPublished { .. }
            | Event::Transferred { .. } => Touch::Set,
            Event::Shared { object_kind, .. } | Event::Revoked { object_kind, .. }
                if object_kind == "document" =>
            {
                Touch::Set
            }
            _ => Touch::None,
        },
        Named::OrgInvitations => match event {
            Event::Invited { .. } | Event::LinkRevoked { .. } | Event::LinkClaimed { .. } => {
                Touch::Set
            }
            _ => Touch::None,
        },
        // The audit tail is append-only and forward-paginated, so a new row is
        // a new key rather than a moved one.
        Named::OrgAudit => match event {
            Event::SignedIn { .. } | Event::SignedOut { .. } => Touch::None,
            _ => Touch::Set,
        },
        _ => Touch::None,
    }
}

/// The overview carries counts, so anything that could move one moves it.
fn overview_touch(event: &Event, party: i64, mine: impl Fn(Id<Group>) -> bool) -> Touch {
    match event {
        Event::OrganizationCreated { organization, .. } if organization.get() == party => {
            Touch::Set
        }
        Event::RoleSet { container, .. }
        | Event::MemberRemoved { container, .. }
        | Event::Left { container, .. }
        | Event::LinkClaimed { container, .. }
            if mine(*container) =>
        {
            Touch::Set
        }
        Event::PartyDisabled { party: it } | Event::PartyEnabled { party: it }
            if it.get() == party =>
        {
            Touch::Set
        }
        Event::GroupCreated { .. } | Event::DocumentCreated { .. } | Event::Transferred { .. } => {
            Touch::Set
        }
        _ => Touch::None,
    }
}

/// Membership rows are keyed by party, and the events that move them name it —
/// but only for the container the subscription is actually listing.
fn membership_touch(event: &Event, params: &Params, key: &IdKey, party: i64) -> Touch {
    let listed = match params.group.as_ref() {
        Some(named) => match ids::decode::<Group>(key, named) {
            Ok(group) => group.get(),
            Err(_) => return Touch::None,
        },
        None => party,
    };
    match event {
        Event::RoleSet {
            container,
            party: who,
        }
        | Event::MemberRemoved {
            container,
            party: who,
        }
        | Event::Left {
            container,
            party: who,
        } if container.get() == listed => Touch::Keys(vec![who.public(key).as_str().to_owned()]),
        Event::LinkClaimed {
            container,
            party: who,
            ..
        } if container.get() == listed => Touch::Keys(vec![who.public(key).as_str().to_owned()]),
        // A party is disabled deployment-wide and the event says nothing
        // about which containers it belonged to, so naming its key here
        // would hand this organization's reader the public id of a party
        // that may be nobody's member. The whole set is re-read instead and
        // diffed against what this connection sent, which is narrower: a
        // `del` can then only mention a row the reader already had.
        Event::PartyDisabled { .. } | Event::PartyEnabled { .. } => Touch::Set,
        _ => Touch::None,
    }
}
