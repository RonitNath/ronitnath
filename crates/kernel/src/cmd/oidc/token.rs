//! The protocol commands: consent, exchange, refresh, service tokens and
//! revocation.
//!
//! Each one is authorised by what its arguments carry rather than by the route
//! that reached it. `authorize` is a person's — it is their session the code
//! is bound to and their consent it records. The other four are a *client's*,
//! and the client proves itself here, by the method its registration names,
//! before anything else is read.
//!
//! The secrets ride on the return type, never in the audit payload and never
//! in a `/api/cmd` reply: a code, an access token and a refresh token exist in
//! the clear exactly once, in the answer to the call that minted them.

use rn_api::commands::{Authorize, ClientCredentials, ExchangeCode, RefreshToken, RevokeToken};
use rn_api::oidc::{GrantType, Scope};

use super::mint::{
    Granted, Grantee, auth_time_of, authenticated_client, harvest, mint_pair, write_pair,
};
use super::{admits_person, client_of};
use crate::bind;
use crate::cmd::{Applied, Batch, Ctx, member, refs, run};
use crate::domain::Token;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::oidc::{code, consent, token as tokens};
use crate::relation::{self, Object, Relation, Subject};
use crate::store::{Sql, Value};

const AUTHORIZE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'authorize', $2, $3, $4, $5, \
             json_object('event', 'authorize', 'client', $6, 'person', $7))";

const EXCHANGE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'exchange-code', $2, $3, $4, $5, \
            json_object('event', 'exchange-code', 'client', $6) WHERE changes() > 0";

const REFRESH_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'refresh-token', $2, $3, $4, $5, \
            json_object('event', 'refresh-token', 'client', $6) WHERE changes() > 0";

const SERVICE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'client-credentials', NULL, NULL, $2, $3, \
             json_object('event', 'client-credentials', 'client', $4, 'service', $5))";

const REVOKE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'revoke-token', NULL, NULL, $2, $3, \
             json_object('event', 'revoke-token', 'client', $4))";

/// What `authorize` produced.
#[derive(Debug)]
pub struct Authorized {
    /// The event and its offset.
    pub committed: Committed,
    /// The code, on the call that minted it and never on a replay.
    pub code: Option<Token>,
}

/// Consent, and mint an authorization code.
///
/// The endpoint has already matched the client and the redirect URI — it has
/// to, because a failure there may not be redirected anywhere — and both are
/// matched again here. That is not belt and braces: this is a command, and a
/// command that trusted its caller's validation would be one an endpoint could
/// forget to do.
pub async fn authorize<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &Authorize,
) -> Outcome<Authorized> {
    let (identity, person, session) = member(&ctx.principal)?;
    let acting_as = refs::acting_as(&ctx.principal, person);
    let client = client_of(ctx, &args.client_id).await?;

    if !client.allows(GrantType::AuthorizationCode)
        || !client.accepts_redirect(&args.redirect_uri)
        || !client.allows_scopes(&args.scopes)
        || !args.scopes.contains(&Scope::Openid)
        || args.code_challenge.is_empty()
    {
        return decline();
    }
    if !admits_person(ctx, &client, person).await? {
        return decline();
    }

    let reads = ctx.store.reads();
    let existing = consent::load(&reads, client.id, person).await?;
    let agreed = consent::widen(existing.as_ref(), &args.scopes);
    let auth_time = auth_time_of(&reads, session).await?;
    let code = Token::mint();
    let now = ctx.now();
    let scopes = Scope::join(&args.scopes);

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        // Consent is a relation row, so `check()` answers "has this person
        // authorised this client" in the one query everything else uses.
        batch.push(relation::grant_stmt(
            Object {
                kind: "oidc_client",
                id: client.id.get(),
            },
            Relation::Authorized,
            Subject::person(person),
            Some(identity),
            now,
        ));
        batch.any(
            consent::UPSERT_SQL,
            bind![client.id, person, Scope::join(&agreed), now],
        );
        batch.one(
            code::INSERT_SQL,
            vec![
                Value::from(code.digest().to_vec()),
                Value::from(client.id),
                Value::from(person),
                Value::from(identity),
                Value::from(session),
                Value::from(args.redirect_uri.as_str()),
                Value::from(scopes.as_str()),
                Value::from(args.nonce.clone()),
                Value::from(args.code_challenge.as_str()),
                Value::from(auth_time),
                Value::from(now + code::TTL),
                Value::from(now),
            ],
        );
        batch.one(
            AUTHORIZE_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(client.id),
                Value::from(person),
            ],
        );
        Ok(batch)
    })
    .await?;

    Ok(Authorized {
        code: applied.fresh.then_some(code),
        committed: applied.committed,
    })
}

/// Exchange a code for tokens.
pub async fn exchange_code<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &ExchangeCode,
) -> Outcome<Granted> {
    let client = authenticated_client(
        ctx,
        &args.client_id,
        args.client_secret.as_deref(),
        args.assertion.as_deref(),
    )
    .await?;
    if !client.allows(GrantType::AuthorizationCode) {
        return decline();
    }
    let reads = ctx.store.reads();
    let now = ctx.now();
    let Some(row) = code::by_secret(&reads, &args.code).await? else {
        return decline();
    };
    // A code that is not this client's, is spent, has expired, or was minted
    // against another landing place: one answer, because telling them apart
    // says which codes exist.
    if row.client_id != client.id || !row.is_live(now) || row.redirect_uri != args.redirect_uri {
        return decline();
    }
    // PKCE. A missing verifier where a challenge was stored is the downgrade
    // this refuses; the schema makes a code with no challenge unrepresentable,
    // so there is no third case.
    let Some(verifier) = args.code_verifier.as_deref() else {
        return decline();
    };
    if !code::verifier_matches(&row.code_challenge, verifier) {
        return decline();
    }

    let scopes = Scope::parse_list(&row.scopes).unwrap_or_default();
    let issued = mint_pair(
        ctx,
        &client,
        Grantee::Person {
            person: row.person_id,
            session: row.session_id,
            auth_time: row.auth_time,
            nonce: row.nonce.clone(),
        },
        &scopes,
        now,
    )
    .await?;

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        // The guarded statement of the batch: a code redeemed twice writes
        // nothing the second time, and every other statement fails with it.
        batch.one(code::REDEEM_SQL, bind![row.id, now]);
        write_pair(&mut batch, &issued);
        batch.one(
            EXCHANGE_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(row.identity_id),
                Value::from(row.person_id),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(client.id),
            ],
        );
        Ok(batch)
    })
    .await?;

    Ok(Granted {
        issued: harvest(applied.fresh, issued),
        committed: applied.committed,
    })
}

/// Rotate a refresh token.
///
/// The one presented dies whatever happens. If it was *already* dead, the
/// whole family dies with it: presenting a rotated-away refresh token is
/// evidence somebody has a copy, and the honest outcome is that both the
/// holder and the thief lose (RFC 9700 §4.14.2). The alternative is that only
/// the thief keeps working.
pub async fn refresh_token<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RefreshToken,
) -> Outcome<Granted> {
    let client = authenticated_client(
        ctx,
        &args.client_id,
        args.client_secret.as_deref(),
        args.assertion.as_deref(),
    )
    .await?;
    if !client.allows(GrantType::RefreshToken) {
        return decline();
    }
    let reads = ctx.store.reads();
    let now = ctx.now();
    let Some(row) = tokens::by_secret(&reads, &args.refresh_token).await? else {
        return decline();
    };
    if row.kind != "refresh" || row.client_id != client.id {
        return decline();
    }
    if !row.is_live(now) {
        // Reuse. Take the family, and refuse.
        if let Some(family) = row.family_id {
            ctx.store
                .execute(tokens::REVOKE_FAMILY_SQL, bind![family, now])
                .await?;
        }
        return decline();
    }
    let (Some(person), Some(session)) = (row.person_id, row.session_id) else {
        return decline();
    };

    // A refresh may narrow what it asks for and never widen it (RFC 6749
    // §6): the person consented to a set, and a token cannot grow past it.
    let held = Scope::parse_list(&row.scopes).unwrap_or_default();
    let asked = match &args.scopes {
        Some(asked) if asked.iter().all(|s| held.contains(s)) => asked.clone(),
        Some(_) => return decline(),
        None => held,
    };

    let mut issued = mint_pair(
        ctx,
        &client,
        Grantee::Person {
            person,
            session,
            auth_time: row.auth_time.unwrap_or(now),
            nonce: row.nonce.clone(),
        },
        &asked,
        now,
    )
    .await?;
    // The new refresh token joins the chain the old one was in, so a reuse
    // anywhere along it takes every link.
    issued.family = row.family_id.or(Some(row.id.get()));

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(tokens::REVOKE_ONE_SQL, bind![row.id, now]);
        write_pair(&mut batch, &issued);
        batch.one(
            REFRESH_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(Option::<i64>::None),
                Value::from(person),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(client.id),
            ],
        );
        Ok(batch)
    })
    .await?;

    Ok(Granted {
        issued: harvest(applied.fresh, issued),
        committed: applied.committed,
    })
}

/// Mint a token whose principal is the client's own service party.
///
/// This is the first route in the deployment that accepts the `service` party
/// kind. What it speaks as is a party like any other, so what it may *do* is
/// whatever relations somebody granted it — there is no implicit authority in
/// being a service.
pub async fn client_credentials<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &ClientCredentials,
) -> Outcome<Granted> {
    let client = authenticated_client(
        ctx,
        &args.client_id,
        args.client_secret.as_deref(),
        args.assertion.as_deref(),
    )
    .await?;
    if !client.allows(GrantType::ClientCredentials) || !client.allows_scopes(&args.scopes) {
        return decline();
    }
    // A public client cannot hold a secret, so it cannot be one: `none` here
    // would be an access token for the asking.
    if client.metadata.token_endpoint_auth_method == rn_api::oidc::ClientAuthMethod::None {
        return decline();
    }
    let now = ctx.now();
    let issued = mint_pair(
        ctx,
        &client,
        Grantee::Service {
            party: client.service_party_id,
        },
        &args.scopes,
        now,
    )
    .await?;

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        write_pair(&mut batch, &issued);
        batch.one(
            SERVICE_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(client.id),
                Value::from(client.service_party_id),
            ],
        );
        Ok(batch)
    })
    .await?;

    Ok(Granted {
        issued: harvest(applied.fresh, issued),
        committed: applied.committed,
    })
}

/// Revoke a token (RFC 7009).
///
/// The specification is explicit that an unknown token is a success: a `200`
/// for a token this deployment never minted is what stops the endpoint being
/// an oracle for which tokens exist. So the audit row is written either way,
/// and the revocation is what may or may not have happened.
pub async fn revoke_token<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RevokeToken,
) -> Outcome<Committed> {
    let client = authenticated_client(
        ctx,
        &args.client_id,
        args.client_secret.as_deref(),
        args.assertion.as_deref(),
    )
    .await?;
    let now = ctx.now();
    let found = tokens::by_secret(&ctx.store.reads(), &args.token)
        .await?
        .filter(|row| row.client_id == client.id);

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        if let Some(row) = &found {
            // A refresh token takes its rotation family with it; an access
            // token is only itself.
            match row.family_id {
                Some(family) if row.kind == "refresh" => {
                    batch.any(tokens::REVOKE_FAMILY_SQL, bind![family, now]);
                }
                _ => {
                    batch.any(tokens::REVOKE_ONE_SQL, bind![row.id, now]);
                }
            }
        }
        batch.one(
            REVOKE_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
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
