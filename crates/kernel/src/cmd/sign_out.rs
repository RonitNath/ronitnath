//! `SignOut` — end the session the request arrived on.
//!
//! Deleting the row is what ending a session means. There is no revoked flag
//! for a query to forget to filter on, and nothing to sweep afterwards.

use rn_api::commands::SignOut;

use super::{Applied, Batch, Ctx, member, run};
use crate::bind;
use crate::error::Outcome;
use crate::event::Committed;
use crate::feed::Feed;
use crate::store::Sql;

const DELETE: &str = "DELETE FROM session WHERE id = $1 AND identity_id = $2";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'sign-out', $2, $3, $4, $5, \
                            json_object('event', 'sign-out', 'identity', $2, 'session', $6) \
                     WHERE changes() > 0";

/// End the acting session.
pub async fn sign_out<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &SignOut) -> Outcome<Committed> {
    let (identity, acting_as, session) = member(&ctx.principal)?;
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(DELETE, bind![session, identity]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                session
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
