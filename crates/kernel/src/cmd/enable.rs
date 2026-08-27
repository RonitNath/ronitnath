//! `Enable` — put a disabled party back.
//!
//! In practice only a platform operator reaches this, because a disabled party
//! cannot sign in to run it on itself. The self case is still permitted rather
//! than special-cased away: the authorisation rule is "yourself or an
//! operator" for both halves of the lifecycle, and carving an exception here
//! would mean the two commands disagree about who they are for.

use rn_api::commands::Enable;

use super::disable::may_act_on;
use super::{Applied, Batch, Ctx, member, run};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Person};
use crate::store::{Reads, Sql};

const PARTY: &str = "UPDATE party SET status = 'active' WHERE id = $1 AND status = 'disabled'";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'enable', $2, $3, $4, $5, \
                            json_object('event', 'enable', 'party', $6) \
                     WHERE changes() > 0";

/// Move a disabled party back to `active`.
pub async fn enable<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Enable) -> Outcome<Committed> {
    let (identity, acting_as, _) = member(&ctx.principal)?;
    let Ok(target) = ids::decode::<Person>(ctx.store.ids(), &args.party) else {
        return decline();
    };
    if !may_act_on(ctx, acting_as, target).await? {
        return decline();
    }
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(PARTY, bind![target]);
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
