//! `SignOut` — end the session the request arrived on.
//!
//! Deleting the row is what ending a session means. There is no revoked flag
//! for a query to forget to filter on, and nothing to sweep afterwards.

use rn_api::commands::SignOut;

use super::{Applied, Batch, Ctx, member, refs, run};
use crate::bind;
use crate::error::Outcome;
use crate::event::Committed;
use crate::feed::Feed;
use crate::oidc::token as oidc;
use crate::store::Sql;

const DELETE: &str = "DELETE FROM session WHERE id = $1 AND identity_id = $2";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'sign-out', $2, $3, $4, $5, \
                            json_object('event', 'sign-out', 'identity', $2, 'session', $6, \
                                        'clients', json($7)) \
                     WHERE changes() > 0";

/// End the acting session.
pub async fn sign_out<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &SignOut) -> Outcome<Committed> {
    let (identity, person, session) = member(&ctx.principal)?;
    let acting_as = refs::acting_as(&ctx.principal, person);
    let now = ctx.now();
    // The relying parties that were given something under this session, read
    // before the batch takes those rows with it. They ride on the event, so
    // the back-channel logout POSTs happen off the request path.
    let targets = oidc::logout_targets(&ctx.store.reads(), session).await?;
    let clients = oidc::target_ids_json(&targets);

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        // The cascade. A token outliving the session that minted it would be
        // a sign-out that means nothing at the site you signed in to.
        batch.any(oidc::DELETE_SESSION_TOKENS_SQL, bind![session]);
        batch.any(oidc::DELETE_SESSION_CODES_SQL, bind![session]);
        batch.one(DELETE, bind![session, identity]);
        batch.one(
            AUDIT,
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
