//! `ActAs` — speak as an organization, or as yourself again.
//!
//! **Attribution, not authority.** `session.acting_as` is what a command
//! writes on its audit row; it is not what any command authorises against, and
//! [`principal::expand`](crate::principal::expand) does not read it. That is
//! deliberate: if switching a session granted anything, switching it back
//! would take something away, and every authorisation in this crate would have
//! two answers depending on a column a caller controls.
//!
//! So what this command needs to prove is only that the person is entitled to
//! *speak* as the party: an organization they administer, or their own person.
//! A group is refused — a group is granted things and never acts — and so is
//! somebody else's organization.
//!
//! The switch takes effect on the next request rather than on this one. The
//! resolved principal is cached per session digest, and the event's
//! `identity` is what evicts it (`server::sub::invalidate`), which is the same
//! machinery that makes a revocation mean anything.

use rn_api::commands::ActAs;

use super::{Applied, Batch, Ctx, member, run};
use crate::authority::{self, Want};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Id, Organization, Person};
use crate::org::MemberRole;
use crate::store::{Reads, Sql};

/// The guard is the person: a session that changed hands between the read and
/// the write is a session this identity no longer holds.
const SESSION: &str = "UPDATE session SET acting_as = $1 WHERE id = $2 AND identity_id = $3";

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'act-as', $2, $3, $4, $5, \
            json_object('event', 'act-as', 'identity', $2, 'session', $6, 'party', $3) \
     WHERE changes() > 0";

/// Point a session at the party it speaks as.
pub async fn act_as<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &ActAs) -> Outcome<Committed> {
    let (identity, person, session) = member(&ctx.principal)?;
    let party = match args.party.as_ref() {
        // Back to the human. Always allowed: it is who they are.
        None => person,
        Some(named) => {
            let key = ctx.store.ids();
            // A person id is accepted only if it is this person's own, so
            // "act as somebody else" is not a spelling of this command.
            if let Ok(who) = ids::decode::<Person>(key, named) {
                if who != person {
                    return decline();
                }
                who
            } else {
                let Ok(organization) = ids::decode::<Organization>(key, named) else {
                    // A group is refused here as well as by the sentence
                    // above it: a group is granted things and never acts.
                    return decline();
                };
                let container: Id<Person> = Id::new(organization.get());
                // C11.1: an operator may speak as any organization. It is
                // attribution and not authority — `check()` answers exactly
                // what it answered before the switch — so what this admits is
                // an audit trail that reads correctly when an operator works
                // inside a tenant.
                authority::require(
                    ctx,
                    Want::Role {
                        container,
                        role: MemberRole::Admin,
                    },
                )
                .await?;
                container
            }
        }
    };
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(SESSION, bind![party, session, identity]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                party,
                now,
                crate::audit::digest_of(args),
                session
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
