//! `Leave` — remove your own membership.
//!
//! The one refusal is the last owner: a container whose only owner walks out
//! is one nobody can transfer, invite into or administer ever again. The way
//! out of *that* is `Transfer`, which is a decision about who takes over
//! rather than a decision to stop being here.
//!
//! Leaving removes the membership row and nothing else. Relations granted to
//! the person directly are theirs and stay; what they lose is everything that
//! reached them through the container, which is the whole point of expressing
//! it that way.

use rn_api::commands::Leave;

use super::{Batch, Ctx, refs, run};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, Person};
use crate::org::{self, MemberRole};
use crate::store::{Reads, Sql};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'leave', $2, $3, $4, $5, \
            json_object('event', 'leave', 'container', $6, 'party', $3) \
     WHERE changes() > 0";

/// Leave a group or an organization.
pub async fn leave<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Leave) -> Outcome<Committed> {
    let (identity, person) = refs::actor(&ctx.principal)?;
    let container: Id<Person> = Id::new(refs::container(ctx.store.ids(), &args.group)?.get());

    let Some(role) = org::role_of(ctx.store, container, person).await? else {
        return decline();
    };
    if role == MemberRole::Owner && org::owner_count(ctx.store, container).await? <= 1 {
        return decline();
    }
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(org::DELETE_SQL, bind![container, person]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                person,
                now,
                crate::audit::digest_of(args),
                container
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
