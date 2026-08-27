//! The OpenID Provider's commands.
//!
//! Thirteen of them, in two halves that are authorised differently.
//!
//! The **registry** half — `register-client`, `update-client`,
//! `rotate-client-secret`, `delete-client`, `rotate-signing-key` — is operator
//! work, and `set-handle` and `revoke-consent` are a person's own. All of them
//! are authorised by a principal the way every other command in this kernel
//! is.
//!
//! The **protocol** half — `authorize`, `exchange-code`, `refresh-token`,
//! `client-credentials`, `revoke-token`, `end-session` — is authorised by a
//! *credential the arguments carry*: a client secret, or a `private_key_jwt`
//! assertion, verified here rather than by the endpoint that received it. That
//! is what lets them be ordinary commands, bound at `/api/cmd/<name>` like
//! everything else, without the route becoming a second authorisation path: a
//! caller holding a cookie and no client secret gets exactly as far as a
//! caller holding neither.
//!
//! And the secrets they mint — a code, an access token, a refresh token —
//! never ride in a `/api/cmd` reply. They come back on the command's own
//! return type, and the `/oidc/*` endpoints are the only callers that read it.

mod client;
mod handle;
mod keys;
mod mint;
mod retire;
mod session;
mod token;

pub use client::{Registered, delete_client, register_client, rotate_client_secret, update_client};
pub use handle::set_handle;
pub use keys::rotate_signing_key;
pub use mint::{Granted, Issued};
pub use retire::retire_key;
pub use session::{end_session, revoke_consent};
pub use token::{
    Authorized, authorize, client_credentials, exchange_code, refresh_token, revoke_token,
};

use crate::authority::{self, Want};
use crate::cmd::{Ctx, refs};
use crate::error::{Outcome, decline};
use crate::feed::Feed;
use crate::ids::{Id, Identity, OidcClient, Person};
use crate::oidc::{ClientRow, client as registry};
use crate::org::{self, MemberRole};
use crate::store::{Reads, Sql};

/// Who is running one of the registry commands, having proved they may.
///
/// Registration is operator-only today. The *shape* admits an organization
/// owning a client — that is what `owner_party_id` is for — so the day an
/// organization may register its own, this is the one function that changes.
pub(crate) async fn operator<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
) -> Outcome<(Id<Identity>, Id<Person>, Id<Person>)> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    authority::require(ctx, Want::Platform).await?;
    Ok((identity, person, acting_as))
}

/// The client a `client_id` names, or the uniform decline.
///
/// A malformed id, one minted under another deployment's key, one naming a
/// row that never existed and one naming a withdrawn client are the same
/// answer: telling them apart is how a caller enumerates a registry.
pub(crate) async fn client_of<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    client_id: &str,
) -> Outcome<ClientRow> {
    let id = registry::decode_client_id(ctx.store.ids(), client_id)?;
    match registry::load(&ctx.store.reads(), id).await? {
        Some(row) => Ok(row),
        None => decline(),
    }
}

/// Whether this person may administer this client: a platform operator, or an
/// administrator of the organization that owns it.
pub(crate) async fn may_administer<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    person: Id<Person>,
    client: &ClientRow,
) -> Outcome<bool> {
    let ordinary = match client.owner_party_id {
        // A platform-owned client has no owning organization to administer it,
        // so the only clause left is the operator's.
        None => false,
        Some(owner) if owner == person => true,
        Some(owner) => {
            org::holds(
                &ctx.store.reads(),
                Id::new(owner.get()),
                person,
                MemberRole::Admin,
            )
            .await?
        }
    };
    authority::allows(ctx, Want::Settled(ordinary)).await
}

/// Whether a `members_only` client will accept this person.
///
/// The rule is the client's owner: a person with no membership under the
/// owning organization is refused, and for a platform-owned client the
/// membership that counts is the operator relation. A client that is not
/// `members_only` accepts anybody who signs in.
pub(crate) async fn admits_person<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    client: &ClientRow,
    person: Id<Person>,
) -> Outcome<bool> {
    if !client.metadata.members_only {
        return Ok(true);
    }
    let reads = ctx.store.reads();
    match client.owner_party_id {
        None => crate::authority::is_operator(&reads, person).await,
        Some(owner) if owner == person => Ok(true),
        Some(owner) => Ok(org::role_of(&reads, Id::new(owner.get()), person)
            .await?
            .is_some()),
    }
}

/// The public id a client is known by — which is its `client_id`.
pub(crate) fn client_id_of<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, client: Id<OidcClient>) -> String {
    client.public(ctx.store.ids()).to_string()
}
