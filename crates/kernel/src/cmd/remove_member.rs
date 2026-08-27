//! `RemoveMember` — take somebody else's membership away.
//!
//! The same write as [`leave`](super::leave), about a different person, and
//! therefore a different question. Leaving needs no authority over anybody;
//! removing needs authority over *them*, so the rule is about the two roles
//! rather than about one:
//!
//! * the actor administers the container — `admin` or better;
//! * the target holds a role strictly below the actor's, so an admin removes
//!   members and an owner removes admins. An admin who could remove another
//!   admin would be able to remove the person who appointed them;
//! * an owner may be removed only by another owner, and never the last one. A
//!   container with no owner is one nobody can transfer, invite into or
//!   administer again, and the way that happens is somebody tidying up;
//! * nobody removes themselves. That is `Leave`, which carries the last-owner
//!   rule of its own, and having two commands mean it would be two places for
//!   it to be wrong.
//!
//! What goes is the membership row and nothing else. Relations granted to the
//! person directly are theirs and stay; what they lose is everything that
//! reached them through the container.

use rn_api::commands::RemoveMember;

use super::{Batch, Ctx, refs, run};
use crate::authority::{self, Want};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, Person};
use crate::org::{self, MemberRole};
use crate::store::{Reads, Sql};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'remove-member', $2, $3, $4, $5, \
            json_object('event', 'remove-member', 'container', $6, 'party', $7) \
     WHERE changes() > 0";

/// Remove a party from a group or an organization.
pub async fn remove_member<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RemoveMember,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let container: Id<Person> = Id::new(refs::container(ctx.store.ids(), &args.group)?.get());
    let subject = refs::subject(ctx.store.ids(), &args.party)?;
    let target: Id<Person> = Id::new(subject.id);
    if target == person {
        // Leaving is a decision about yourself, and it has its own command.
        return decline();
    }

    let actor_role = org::role_of(ctx.store, container, person).await?;
    let Some(current) = org::role_of(ctx.store, container, target).await? else {
        // Not a membership this container has. Not authorisation either: an
        // operator cannot remove somebody who is not there.
        return decline();
    };
    authority::require(
        ctx,
        Want::Settled(actor_role.is_some_and(|held| may_remove(held, current))),
    )
    .await?;
    if current == MemberRole::Owner && org::owner_count(ctx.store, container).await? <= 1 {
        return decline();
    }
    let now = ctx.now();

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(org::DELETE_SQL, bind![container, target]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                container,
                target
            ],
        );
        batch.push(crate::audit::object_stmt(
            ctx.key,
            crate::audit::object::PARTY,
            container.get(),
        ));
        batch.push(crate::audit::object_stmt(
            ctx.key,
            crate::audit::object::PARTY,
            target.get(),
        ));
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}

/// Whether one role may take another away.
///
/// Strictly below, with one exception written out rather than derived: an
/// owner may remove another owner, because somebody has to be able to and the
/// last-owner rule is what keeps that safe.
const fn may_remove(actor: MemberRole, target: MemberRole) -> bool {
    matches!(
        (actor, target),
        (MemberRole::Owner, _) | (MemberRole::Admin, MemberRole::Member)
    )
}
