//! The fixtures the relation commands' tests share.
//!
//! Everything here goes through the real commands. A fixture that inserted
//! rows directly would be a second way to make an organization, and the tests
//! would then be proving that one.

use rn_api::commands::{CreateDocument, CreateGroup, CreateOrganization, Invite};
use rn_api::ids::PublicId;
use rn_api::whoami::MemberRole;

use crate::cmd;
use crate::domain::Token;
use crate::error::Outcome;
use crate::event::Event;
use crate::ids::{Group, Id, IdKey, Organization, Person, Public, Resource};
use crate::principal::Principal;
use crate::store::Reads;
use crate::testing::Local;

/// Somebody who exists, and the principal their cookie resolves to.
pub(super) struct Who {
    pub(super) principal: Principal,
}

impl Who {
    /// The person they are.
    pub(super) fn person(&self) -> Id<Person> {
        self.principal
            .acting_as()
            .expect("a member acts as somebody")
    }
}

/// Register somebody through the real command.
pub(super) async fn person(harness: &Local, name: &str, email: &str) -> Who {
    let registered = harness.register(name, email).await.expect("registers");
    Who {
        principal: registered.principal,
    }
}

/// The public id of a row, as a command's arguments carry it.
pub(super) fn public<T: Public>(key: &IdKey, id: Id<T>) -> PublicId {
    id.public(key)
}

/// The id key the harness mints ids under.
pub(super) fn key(harness: &Local) -> &IdKey {
    harness.store().ids()
}

/// Found an organization, returning its party id and its resource id.
pub(super) async fn organization(
    harness: &Local,
    who: &Who,
    name: &str,
) -> Outcome<(Id<Organization>, Id<Resource>)> {
    let committed = cmd::create_organization(
        &harness.ctx(who.principal.clone()),
        &CreateOrganization {
            display_name: name.to_owned(),
        },
    )
    .await?;
    match committed.event {
        Event::OrganizationCreated {
            organization,
            resource,
            ..
        } => Ok((organization, resource)),
        other => panic!("create-organization produced {other:?}"),
    }
}

/// Create a group, in an organization or not.
pub(super) async fn group(
    harness: &Local,
    who: &Who,
    name: &str,
    org: Option<Id<Organization>>,
) -> Outcome<Id<Group>> {
    let committed = cmd::create_group(
        &harness.ctx(who.principal.clone()),
        &CreateGroup {
            display_name: name.to_owned(),
            organization: org.map(|id| public(key(harness), id)),
        },
    )
    .await?;
    match committed.event {
        Event::GroupCreated { group, .. } => Ok(group),
        other => panic!("create-group produced {other:?}"),
    }
}

/// Mint an invitation and hand back its token.
pub(super) async fn invite(
    harness: &Local,
    who: &Who,
    container: PublicId,
    role: MemberRole,
    expires_at: i64,
) -> Outcome<Token> {
    let minted = cmd::invite(
        &harness.ctx(who.principal.clone()),
        &Invite {
            group: container,
            role,
            expires_at,
        },
    )
    .await?;
    Ok(minted.token.expect("a fresh mint returns its token"))
}

/// Create a document, returning its resource id.
pub(super) async fn document(
    harness: &Local,
    who: &Who,
    title: &str,
    owner: Option<Id<Organization>>,
) -> Outcome<Id<Resource>> {
    let committed = cmd::create_document(
        &harness.ctx(who.principal.clone()),
        &CreateDocument {
            title: title.to_owned(),
            body: "Five rules".to_owned(),
            owner: owner.map(|id| public(key(harness), id)),
        },
    )
    .await?;
    match committed.event {
        Event::DocumentCreated { document, .. } => Ok(document),
        other => panic!("create-document produced {other:?}"),
    }
}

/// How many rows a `SELECT count(*) AS n` finds.
pub(super) async fn count(
    harness: &Local,
    sql: &'static str,
    params: Vec<crate::store::Value>,
) -> i64 {
    harness
        .store()
        .query::<crate::store::Count>(sql, params)
        .await
        .expect("the query runs")[0]
        .0
}
