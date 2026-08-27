//! The client registry's four commands.
//!
//! Registering is operator work today, and the row is shaped like an RFC 7591
//! registration request anyway — so the day dynamic registration is wanted,
//! the endpoint has nothing to migrate and this file gains an authorisation
//! rule rather than a schema.
//!
//! Two things are minted alongside the row and both are on purpose. A
//! **secret**, once, returned only on the call that made it — the row keeps
//! its SHA-256, exactly as a session does. And a **service party**, so that
//! `client_credentials` has a principal to speak as from the moment the client
//! exists rather than one conjured at the first token.

use rn_api::commands::{DeleteClient, RegisterClient, RotateClientSecret, UpdateClient};
use rn_api::oidc::{ClientAuthMethod, ClientMetadata, GrantType, Scope};

use super::{client_of, may_administer, operator};
use crate::cmd::{Applied, Batch, Ctx, refs, run};
use crate::domain::Token;
use crate::error::{Invalid, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Id, Organization, Person};
use crate::oidc::client as registry;
use crate::store::{Reads, Sql, Value};

const REGISTER_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'register-client', $2, $3, $4, $5, \
             json_object('event', 'register-client', 'client', $6, 'owner', $7))";

const UPDATE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'update-client', $2, $3, $4, $5, \
            json_object('event', 'update-client', 'client', $6) WHERE changes() > 0";

const ROTATE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'rotate-client-secret', $2, $3, $4, $5, \
            json_object('event', 'rotate-client-secret', 'client', $6) WHERE changes() > 0";

const DELETE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'delete-client', $2, $3, $4, $5, \
            json_object('event', 'delete-client', 'client', $6) WHERE changes() > 0";

/// What a registration produced.
#[derive(Debug)]
pub struct Registered {
    /// The event and its offset.
    pub committed: Committed,
    /// The client's secret, on the call that minted it and never on a replay.
    /// `None` for a public client, which has none.
    pub secret: Option<Token>,
}

/// Check a registration's metadata before anything is written.
///
/// Every one of these is a shape complaint about what the caller sent, so it
/// is an [`Invalid`] and says which field — the operator registering a client
/// is the person who can fix it.
fn validate(metadata: &ClientMetadata) -> Result<(), Invalid> {
    if metadata.client_name.trim().is_empty() {
        return Err(Invalid::Missing("client_name"));
    }
    if metadata.client_name.chars().count() > crate::domain::DISPLAY_NAME_LIMIT {
        return Err(Invalid::TooLong {
            field: "client_name",
            limit: crate::domain::DISPLAY_NAME_LIMIT,
        });
    }
    // A client that may run the code flow has to say where a code may land,
    // and one that may not has no use for a redirect.
    if metadata.grant_types.contains(&GrantType::AuthorizationCode)
        && metadata.redirect_uris.is_empty()
    {
        return Err(Invalid::Missing("redirect_uris"));
    }
    if metadata.grant_types.is_empty() {
        return Err(Invalid::Missing("grant_types"));
    }
    if !metadata.scopes.contains(&Scope::Openid)
        && metadata.grant_types.contains(&GrantType::AuthorizationCode)
    {
        return Err(Invalid::Missing("scopes"));
    }
    // A signature this deployment cannot check is a client that cannot
    // authenticate, and it would fail at the token endpoint instead of here.
    if metadata.token_endpoint_auth_method == ClientAuthMethod::PrivateKeyJwt
        && metadata.jwks.is_none()
    {
        return Err(Invalid::Missing("jwks"));
    }
    for uri in metadata
        .redirect_uris
        .iter()
        .chain(&metadata.post_logout_redirect_uris)
        .chain(metadata.backchannel_logout_uri.as_ref())
    {
        // Absolute, and with no fragment: RFC 6749 §3.1.2 forbids a fragment
        // on a redirect, and a relative URI names this deployment, which is
        // never where a relying party lives.
        if !(uri.starts_with("https://") || uri.starts_with("http://")) || uri.contains('#') {
            return Err(Invalid::Missing("redirect_uris"));
        }
    }
    Ok(())
}

/// The columns a registration binds, in the order [`registry::INSERT_SQL`]
/// names them — apart from the owner, the service party and the secret, which
/// the two callers differ on.
fn metadata_values(metadata: &ClientMetadata) -> Vec<Value> {
    vec![
        Value::from(metadata.client_name.trim()),
        Value::from(metadata.client_uri.clone()),
        Value::from(metadata.logo_uri.clone()),
        Value::from(registry::strings_as_json(&metadata.redirect_uris)),
        Value::from(registry::strings_as_json(
            &metadata.post_logout_redirect_uris,
        )),
        Value::from(metadata.backchannel_logout_uri.clone()),
        Value::from(metadata.token_endpoint_auth_method.as_str()),
        Value::from(metadata.jwks.clone()),
        Value::from(registry::as_json(&metadata.grant_types, GrantType::as_str)),
        Value::from(registry::as_json(&metadata.scopes, Scope::as_str)),
        Value::from(metadata.trusted),
        Value::from(metadata.members_only),
    ]
}

/// Register a relying party.
pub async fn register_client<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RegisterClient,
) -> Outcome<Registered> {
    let (identity, _, acting_as) = operator(ctx).await?;
    validate(&args.metadata)?;

    // An owner is an organization, or nobody — which is the deployment
    // itself. A person is refused here rather than at the schema's trigger:
    // the sector a person's own clients would be under is that person, and
    // "your clients see you as yourself" is a design nobody has asked for.
    let owner: Option<Id<Person>> = match &args.owner {
        None => None,
        Some(id) => match ids::decode::<Organization>(ctx.store.ids(), id) {
            Ok(org) => Some(Id::new(org.get())),
            Err(_) => return decline(),
        },
    };
    let secret = args
        .metadata
        .token_endpoint_auth_method
        .uses_secret()
        .then(registry::mint_secret);
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        // The service party the client's own tokens speak as, created with the
        // client so `client_credentials` never has to conjure one.
        let service = batch.one(
            registry::SERVICE_PARTY_SQL,
            vec![
                Value::from(format!("{} (service)", args.metadata.client_name.trim())),
                Value::from(now),
            ],
        );
        let mut params = vec![Value::from(owner), service.column("id")];
        params.extend(metadata_values(&args.metadata));
        params.push(Value::from(
            secret.as_ref().map(|s| registry::secret_digest(s.expose())),
        ));
        params.push(Value::from(now));
        let client = batch.one(registry::INSERT_SQL, params);
        batch.one(
            REGISTER_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                client.column("id"),
                Value::from(owner),
            ],
        );
        Ok(batch)
    })
    .await?;

    Ok(Registered {
        secret: applied.fresh.then_some(secret).flatten(),
        committed: applied.committed,
    })
}

/// Replace a client's metadata.
pub async fn update_client<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &UpdateClient,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    validate(&args.metadata)?;
    let id = crate::ids::decode::<crate::ids::OidcClient>(ctx.store.ids(), &args.client)
        .map_err(|_| crate::error::Decline)?;
    let Some(client) = registry::load(&ctx.store.reads(), id).await? else {
        return decline();
    };
    if !may_administer(ctx, person, &client).await? {
        return decline();
    }
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        let mut params = vec![Value::from(id)];
        params.extend(metadata_values(&args.metadata));
        batch.one(registry::UPDATE_SQL, params);
        batch.one(
            UPDATE_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(id),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// Mint a new secret. The old one stops working in the same statement.
pub async fn rotate_client_secret<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RotateClientSecret,
) -> Outcome<Registered> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let client = client_of(ctx, args.client.as_str()).await?;
    if !may_administer(ctx, person, &client).await? {
        return decline();
    }
    // A public client has no secret to rotate, and minting one would make it
    // confidential without anybody saying so.
    if !client.metadata.token_endpoint_auth_method.uses_secret() {
        return decline();
    }
    let secret = registry::mint_secret();
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(
            registry::ROTATE_SECRET_SQL,
            vec![
                Value::from(client.id),
                Value::from(registry::secret_digest(secret.expose())),
                Value::from(now),
            ],
        );
        batch.one(
            ROTATE_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(client.id),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(Registered {
        secret: applied.fresh.then_some(secret),
        committed: applied.committed,
    })
}

/// Withdraw a client, and everything it holds.
///
/// A tombstone rather than a `DELETE`: codes, tokens and consents reference
/// the row, and the audit trail references them. What goes is what the client
/// could *do* — its tokens are revoked, its consents deleted, and the relation
/// rows that were those consents deleted with them.
pub async fn delete_client<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &DeleteClient,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let client = client_of(ctx, args.client.as_str()).await?;
    if !may_administer(ctx, person, &client).await? {
        return decline();
    }
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(
            registry::DELETE_SQL,
            vec![Value::from(client.id), Value::from(now)],
        );
        batch.any(
            registry::REVOKE_CLIENT_TOKENS_SQL,
            vec![Value::from(client.id), Value::from(now)],
        );
        batch.any(
            registry::DELETE_CLIENT_CONSENTS_SQL,
            vec![Value::from(client.id)],
        );
        batch.any(
            registry::DELETE_CLIENT_GRANTS_SQL,
            vec![Value::from(client.id)],
        );
        batch.one(
            DELETE_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(client.id),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
