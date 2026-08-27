//! `Revoke` — withdraw exactly the row that was written.
//!
//! Exactly that row, and no other path the subject may also hold. If Ronit can
//! see a document both directly and through a group, revoking the direct grant
//! leaves the group one alone — because the group's grant is a different
//! decision, made by somebody who is not being asked here.
//!
//! Revoking something that was never granted is not an error. The world the
//! caller asked for is the world that already exists, so the command succeeds
//! and the audit row records that it ran.
//!
//! A platform operator may withdraw any relation at all. That is not a hole in
//! the rule above — it is the rule that a deployment has somebody who can
//! answer for it, and the audit row names them, which is what makes the power
//! reviewable rather than merely present.

use rn_api::commands::Revoke;

use super::share::{may_share, object_of, relation_of};
use super::{Batch, Ctx, refs, run};
use crate::authority::{self, Want};
use crate::error::Outcome;
use crate::event::Committed;
use crate::feed::Feed;
use crate::relation;
use crate::store::{Reads, Sql, Value};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'revoke', $2, $3, $4, $5, \
             json_object('event', 'revoke', 'object_kind', $6, 'object_id', $7, \
                         'relation', $8))";

/// Withdraw a relation on a resource.
pub async fn revoke<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Revoke) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let object = object_of(ctx, &args.resource).await?;
    let subject = refs::subject(ctx.store.ids(), &args.subject)?;
    let relation = relation_of(args.relation);

    let mine = may_share(ctx, object, person).await?;
    authority::require(ctx, Want::Settled(mine)).await?;
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.push(relation::revoke_stmt(object, relation, subject));
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(object.kind),
                Value::from(object.id),
                Value::from(relation.as_str()),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
