//! `RevokeSession` — end some other session of the same identity.
//!
//! The authorisation is the `AND identity_id = $2` in the delete, and it is
//! there rather than in a prior read on purpose: a check that reads and then
//! deletes has a window between the two, and the window is exactly long enough
//! for the row to become somebody else's. A session id belonging to another
//! identity matches nothing, the guard misses, and the caller is declined —
//! the same decline an id that never existed gets.

use rn_api::commands::RevokeSession;

use super::{Applied, Batch, Ctx, member, run};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids;
use crate::store::{Reads, Sql};

const DELETE: &str = "DELETE FROM session WHERE id = $1 AND identity_id = $2";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'revoke-session', $2, $3, $4, $5, \
                            json_object('event', 'revoke-session', 'identity', $2, \
                                        'session', $6) \
                     WHERE changes() > 0";

/// End another session of the acting identity — signing out a lost device.
pub async fn revoke_session<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RevokeSession,
) -> Outcome<Committed> {
    let (identity, acting_as, _) = member(&ctx.principal)?;
    let Ok(target) = ids::decode::<ids::Session>(ctx.store.ids(), &args.session) else {
        // A forged or foreign id is refused without a query, and refused the
        // same way a real id belonging to somebody else would be.
        return decline();
    };
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(DELETE, bind![target, identity]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                target
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
