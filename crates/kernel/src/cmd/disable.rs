//! `Disable` — stop a party authenticating and being a usable subject.
//!
//! Not deletion. The model does not delete a party: rows across every product
//! reference it, `person_alias` keeps its old URLs resolving, and a merge may
//! yet absorb it. Disabling is reversible by [`enable`](super::enable), and
//! the two together are the whole lifecycle.
//!
//! Two principals may run it: the person themselves, and a platform operator.
//! "Platform operator" is a relation row (`platform:* #operator @person:R`),
//! not a column and not a role enum — the kernel report's ruling, and the
//! reason nothing in this crate has an `is_admin`.

use rn_api::commands::Disable;

use super::{Applied, Batch, Ctx, is_platform_operator, member, run};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Id, Person};
use crate::store::{Reads, Sql};

const PARTY: &str = "UPDATE party SET status = 'disabled' WHERE id = $1 AND status = 'active'";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'disable', $2, $3, $4, $5, \
                            json_object('event', 'disable', 'party', $6, 'reason', $7) \
                     WHERE changes() > 0";

/// Every session of every identity resolved onto the party goes with it — a
/// disabled party that keeps answering from an open tab is not disabled. This
/// runs after the audit row, because it is a consequence of the decision
/// rather than part of it, and its count carries no information.
const SESSIONS: &str = "DELETE FROM session WHERE identity_id IN \
                        (SELECT id FROM identity WHERE person_id = $1 OR id = $1)";

/// Move a party to `disabled`.
pub async fn disable<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Disable) -> Outcome<Committed> {
    let (identity, acting_as, _) = member(&ctx.principal)?;
    let Ok(target) = ids::decode::<Person>(ctx.store.ids(), &args.party) else {
        // An organization or a group id lands here too: K2 owns those, and a
        // command that half-understood one would be worse than a decline.
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
                target,
                args.reason.as_str()
            ],
        );
        batch.any(SESSIONS, bind![target]);
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// Yourself, or anybody if you are a platform operator.
pub(super) async fn may_act_on<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    acting_as: Id<Person>,
    target: Id<Person>,
) -> Outcome<bool> {
    Ok(acting_as == target || is_platform_operator(ctx.store, acting_as).await?)
}
