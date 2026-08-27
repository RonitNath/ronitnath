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

use rn_api::commands::Revoke;

use super::share::{SHARER, object_of, relation_of};
use super::{Batch, Ctx, refs, run};
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::principal::expand;
use crate::relation;
use crate::store::{Reads, Sql, Value};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     VALUES ($1, 'revoke', $2, $3, $4, $5, \
             json_object('event', 'revoke', 'resource', $6, 'relation', $7))";

/// Withdraw a relation on a resource.
pub async fn revoke<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Revoke) -> Outcome<Committed> {
    let (identity, person) = refs::actor(&ctx.principal)?;
    let object = object_of(ctx, &args.resource).await?;
    let subject = refs::subject(ctx.store.ids(), &args.subject)?;
    let relation = relation_of(args.relation);

    let subjects = expand(ctx.store, &ctx.principal).await?;
    if !relation::check(ctx.store, &subjects, SHARER, object).await? {
        return decline();
    }
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.push(relation::revoke_stmt(object, relation, subject));
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(identity),
                Value::from(person),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(object.id),
                Value::from(relation.as_str()),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
