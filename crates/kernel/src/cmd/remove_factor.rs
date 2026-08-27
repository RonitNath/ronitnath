//! `RemoveFactor` — take a factor off the acting identity.
//!
//! The rule the guard encodes is that an identity never loses its last way in.
//! Removing the only password, or the only email, is refused — not to be
//! protective, but because the alternative is a row nobody can authenticate
//! and nobody can delete, which the model has no command to repair.

use rn_api::commands::RemoveFactor;

use super::{Applied, Batch, Ctx, refs, run};
use crate::authority::{self, Want};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids;
use crate::merge::target_identity;
use crate::store::{Reads, Sql};

/// The delete carries the whole rule: it is the acting identity's factor, and
/// at least one other factor of the same kind survives it.
const DELETE: &str = "DELETE FROM factor \
                      WHERE id = $1 AND identity_id = $2 \
                        AND EXISTS (SELECT 1 FROM factor other \
                                    WHERE other.identity_id = $2 AND other.id <> $1 \
                                      AND other.kind = (SELECT kind FROM factor WHERE id = $1))";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'remove-factor', $2, $3, $4, $5, \
                            json_object('event', 'remove-factor', 'identity', $6, 'factor', $7) \
                     WHERE changes() > 0";

/// Remove a factor, provided it is not the identity's last of its kind.
pub async fn remove_factor<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RemoveFactor,
) -> Outcome<Committed> {
    // An impersonated session may not change what the person is, or who
    // may become them (`authority::FORBIDDEN_WHILE_IMPERSONATING`).
    authority::not_impersonating(&ctx.principal)?;
    let (actor, person, acting_as) = refs::actor(&ctx.principal)?;
    let identity =
        match target_identity(&ctx.store.reads(), actor, person, args.identity.as_ref()).await {
            Ok(identity) => identity,
            Err(error) if error.is_decline() => {
                authority::require(ctx, Want::Platform).await?;
                super::add_factor::any_identity(ctx, args.identity.as_ref()).await?
            }
            Err(error) => return Err(error),
        };
    let Ok(target) = ids::decode::<ids::Factor>(ctx.store.ids(), &args.factor) else {
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
                actor,
                acting_as,
                now,
                crate::audit::digest_of(args),
                identity,
                target
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
