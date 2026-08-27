//! `SetRole` — move a member between `member` and `admin`.
//!
//! Three refusals, and each is a rule rather than a precaution:
//!
//! * **nobody reaches `owner` through here.** Ownership moves through
//!   `Transfer`, which is also what keeps `resource.owner_party_id` and the
//!   owner membership agreeing.
//! * **only an owner may change an owner's role.** An admin who could demote
//!   an owner would be an owner with extra steps.
//! * **the last owner cannot be demoted.** A container with no owner is one
//!   nobody can transfer, invite into, or dissolve, and the way that happens
//!   is somebody tidying up their own role.

use rn_api::commands::SetRole;

use super::{Batch, Ctx, refs, run};
use crate::bind;
use crate::domain::Vocabulary as _;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, Person};
use crate::org::{self, MemberRole};
use crate::store::{Reads, Sql};

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'set-role', $2, $3, $4, $5, \
            json_object('event', 'set-role', 'container', $6, 'party', $7) \
     WHERE changes() > 0";

/// Change a party's role in a group or an organization.
pub async fn set_role<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &SetRole) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let container: Id<Person> = Id::new(refs::container(ctx.store.ids(), &args.group)?.get());
    let subject = refs::subject(ctx.store.ids(), &args.party)?;
    let target: Id<Person> = Id::new(subject.id);
    let wanted = MemberRole::from(args.role);
    if wanted == MemberRole::Owner {
        return decline();
    }

    let Some(actor_role) = org::role_of(ctx.store, container, person).await? else {
        return decline();
    };
    if !actor_role.covers(MemberRole::Admin) {
        return decline();
    }
    let Some(current) = org::role_of(ctx.store, container, target).await? else {
        return decline();
    };
    if current == MemberRole::Owner {
        if actor_role != MemberRole::Owner {
            return decline();
        }
        if org::owner_count(ctx.store, container).await? <= 1 {
            return decline();
        }
    }

    let now = ctx.now();
    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        // Guarded on the role that was read: a role changed by somebody else
        // in between writes nothing rather than overwriting their decision.
        batch.one(
            org::SET_ROLE_SQL,
            bind![wanted.as_str(), container, target, current.as_str()],
        );
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
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
