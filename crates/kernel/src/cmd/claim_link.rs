//! `ClaimLink` — hold the token, be signed in, become a member.
//!
//! Two things have to be true and they are different things. The **token**
//! says what is being joined and at what role; the **session** says who is
//! joining. Neither substitutes for the other: a bearer with no session has
//! nobody to make a member of, and a session with no token has no invitation.
//! That is why this command is the one place a bearer principal and a signed-in
//! identity meet.
//!
//! Single use is the database's job. Every statement in the batch carries the
//! same guard — the link is unclaimed and unexpired — so a token claimed twice
//! writes nothing the second time rather than half of something.

use rn_api::commands::ClaimLink;

use super::{Batch, Ctx, refs, run};
use crate::bind;
use crate::domain::{Token, Vocabulary as _};
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::invite;
use crate::store::Sql;

/// The membership, guarded on the link. `ON CONFLICT DO NOTHING`, because a
/// person who is already a member keeps the role they have — an invitation
/// does not demote anybody.
const MEMBERSHIP: &str = "INSERT INTO membership (group_id, party_id, role, at) \
     SELECT $1, $2, $3, $4 \
     WHERE EXISTS (SELECT 1 FROM link WHERE id = $5 \
                   AND claimed_by_identity_id IS NULL AND expires_at > $4) \
     ON CONFLICT (group_id, party_id) DO NOTHING";

/// `party` is the person the membership landed on — `$7`. `$3` is the
/// `acting_as` column, which after `ActAs` is a different party altogether,
/// and naming it here made the feed report that an organization had joined a
/// group one of its members was invited to.
const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'claim-link', $2, $3, $4, $5, \
            json_object('event', 'claim-link', 'container', $6, 'identity', $2, 'party', $7) \
     WHERE changes() > 0";

/// Claim an invitation.
pub async fn claim_link<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &ClaimLink,
) -> Outcome<Committed> {
    let (actor, person, acting_as) = refs::actor(&ctx.principal)?;
    // The session's *own* identity, not the actor's: `claimed_by_identity_id`
    // records who now holds the invitation, and under impersonation the actor
    // is the operator while the registration joining the container is the
    // target's. The audit row keeps the actor, which is the other half of the
    // same distinction.
    let (identity, _, _) = super::member(&ctx.principal)?;
    let token = Token::from_wire(&args.token);
    let now = ctx.now();

    // Unknown, expired and already claimed are one answer: a token that does
    // not work now. Telling them apart would say which invitations exist.
    let Some(invitation) = invite::lookup(ctx.store, token.digest())
        .await?
        .filter(|found| found.is_claimable(now))
    else {
        return decline();
    };

    let applied = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.any(
            MEMBERSHIP,
            bind![
                invitation.container,
                person,
                invitation.role.as_str(),
                now,
                invitation.id()
            ],
        );
        // The guarded statement of the batch: if this affects nothing, the
        // link was claimed or expired between the read and here, every other
        // statement's guard failed with it, and `run` retries once.
        batch.one(invite::CLAIM_SQL, bind![identity, now, invitation.id()]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                actor,
                acting_as,
                now,
                crate::audit::digest_of(args),
                invitation.container,
                person
            ],
        );
        batch.push(crate::audit::object_stmt(
            ctx.key,
            crate::audit::object::PARTY,
            invitation.container.get(),
        ));
        batch.push(crate::audit::object_stmt(
            ctx.key,
            crate::audit::object::LINK,
            invitation.id().get(),
        ));
        Ok(batch)
    })
    .await?;
    Ok(applied.committed)
}
