//! Ending a session at a relying party's request, and withdrawing a consent.
//!
//! Both are the same sentence from two directions: something the person gave a
//! client is being taken back, and everything it produced goes with it in the
//! same transaction. A consent that could be withdrawn while its tokens kept
//! working would be a button that does nothing.

use rn_api::commands::{EndSession, RevokeConsent};

use super::client_of;
use crate::bind;
use crate::cmd::{Applied, Batch, Ctx, member, refs, run};
use crate::error::Outcome;
use crate::event::Committed;
use crate::feed::Feed;
use crate::oidc::{consent, token as tokens};
use crate::relation::{self, Object, Relation, Subject};
use crate::store::{Sql, Value};

const END_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'end-session', $2, $3, $4, $5, \
            json_object('event', 'end-session', 'identity', $2, 'session', $6, \
                        'clients', json($7)) \
     WHERE changes() > 0";

const CONSENT_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'revoke-consent', $2, $3, $4, $5, \
             json_object('event', 'revoke-consent', 'client', $6, 'person', $7))";

const DELETE_SESSION: &str = "DELETE FROM session WHERE id = $1 AND identity_id = $2";

/// End the acting session because a relying party asked.
///
/// The same cascade as `sign-out` and a different word on the audit row,
/// because the row's job is to say what happened: somebody pressed "sign out"
/// at another site and this deployment obliged.
pub async fn end_session<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &EndSession,
) -> Outcome<Committed> {
    let (identity, person, session) = member(&ctx.principal)?;
    let acting_as = refs::acting_as(&ctx.principal, person);
    let now = ctx.now();
    // Read before the batch: the batch takes these rows with it, so afterwards
    // there is nothing left to ask.
    let targets = tokens::logout_targets(&ctx.store.reads(), session).await?;
    let clients = tokens::target_ids_json(&targets);

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.any(tokens::DELETE_SESSION_TOKENS_SQL, bind![session]);
        batch.any(tokens::DELETE_SESSION_CODES_SQL, bind![session]);
        batch.one(DELETE_SESSION, bind![session, identity]);
        batch.one(
            END_AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                session,
                clients.as_str()
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// Withdraw a consent, and every token it produced.
pub async fn revoke_consent<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RevokeConsent,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    // A person withdraws their own consent and nobody else's, which needs no
    // rule beyond "the row is keyed on them": there is no id here naming
    // somebody else's.
    let client = client_of(ctx, args.client.as_str()).await?;
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.push(relation::revoke_stmt(
            Object {
                kind: "oidc_client",
                id: client.id.get(),
            },
            Relation::Authorized,
            Subject::person(person),
        ));
        batch.any(consent::DELETE_SQL, bind![client.id, person]);
        batch.any(
            tokens::REVOKE_CONSENT_SQL,
            vec![
                Value::from(client.id),
                Value::from(person),
                Value::from(now),
            ],
        );
        batch.one(
            CONSENT_AUDIT,
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
    Ok(committed)
}
