//! `GrantOperator` and `RevokeOperator` — delegating full authority, and
//! taking it back.
//!
//! Finding **F2**: until this file, the only way to become an operator was the
//! bootstrap, which refuses the second one
//! ([`crate::merge::rule::make_operator`]). The comment in
//! [`crate::cmd`] said `SetRole` wrote the row and it could not:
//! `set_role` writes `membership` through a *container*, and `platform` is not
//! a container ([`crate::relation::kinds`], `is_container: false`). So story A2
//! — delegate operator authority and take it back — was unimplementable as
//! documented rather than merely unimplemented, and a deployment with one
//! operator had no way to have two.
//!
//! Three things make delegation different from the bootstrap and each is
//! written into the command.
//!
//! * **It has an actor**, so it writes an audit row. The bootstrap does not
//!   and must not: an unaudited grant is defensible as the act that creates
//!   the first operator out of nothing and indefensible as the act that
//!   creates the second.
//! * **The reason is mandatory**, bounded like `RuleMatch`'s evidence. A
//!   delegation of full authority with no stated reason is a row nobody can
//!   account for later, and "later" is the only time anybody will look.
//! * **The last operator cannot be revoked.** The same shape as the last-owner
//!   rule, and a stronger reason: the bootstrap only fires on a deployment
//!   with *no* operator, so revoking the last one is recoverable — and only by
//!   somebody with the database open and the process stopped. Refusing is
//!   cheaper than that.
//!
//! Promotion also shortens what it promotes. A person holding a fourteen-day
//! session who is granted the relation would be holding a fourteen-day
//! god-mode cookie, so every live session of theirs is cut to
//! [`OPERATOR_SESSION_TTL`] in the same transaction (requirement A3.1).

use rn_api::commands::{GrantOperator, RevokeOperator};

use super::{Applied, Batch, Ctx, refs, run};
use crate::audit::{self, object};
use crate::authority::{self, Want};
use crate::bind;
use crate::domain::OPERATOR_SESSION_TTL;
use crate::error::{Invalid, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Id, Person};
use crate::merge::rule::EVIDENCE_LIMIT;
use crate::store::{Reads, Sql};

/// The grant. `INSERT ... WHERE NOT EXISTS` rather than `INSERT OR IGNORE`,
/// because the guarded count is what tells `run` the world moved: granting a
/// relation somebody already holds writes nothing and, with it, no audit row —
/// which is right, since nothing happened.
const GRANT: &str = "INSERT INTO relation \
     (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
     SELECT 'platform', 0, 'operator', 'person', $1, $2, $3 \
     WHERE NOT EXISTS (SELECT 1 FROM relation \
                       WHERE object_kind = 'platform' AND object_id = 0 \
                         AND relation = 'operator' AND subject_kind = 'person' \
                         AND subject_id = $1)";

/// A promotion shortens what it promotes: every live session of the new
/// operator is cut to the operator TTL. `any`, because they may hold none.
const SHORTEN: &str = "UPDATE session SET expires_at = $1 \
     WHERE expires_at > $1 \
       AND identity_id IN (SELECT id FROM identity WHERE person_id = $2)";

/// The withdrawal. The count in the guard is the last-operator rule restated
/// where it is transactional: a concurrent revoke that took the other one
/// makes this write nothing rather than emptying the deployment.
const REVOKE: &str = "DELETE FROM relation \
     WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator' \
       AND subject_kind = 'person' AND subject_id = $1 \
       AND (SELECT count(*) FROM relation \
            WHERE object_kind = 'platform' AND object_id = 0 \
              AND relation = 'operator') > 1";

const GRANT_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'grant-operator', $2, $3, $4, $5, \
            json_object('event', 'grant-operator', 'person', $6, 'reason', $7) \
     WHERE changes() > 0";

const REVOKE_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'revoke-operator', $2, $3, $4, $5, \
            json_object('event', 'revoke-operator', 'person', $6, 'reason', $7) \
     WHERE changes() > 0";

/// Grant `platform:* #operator` to a person.
pub async fn grant_operator<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &GrantOperator,
) -> Outcome<Committed> {
    authority::not_impersonating(&ctx.principal)?;
    let (identity, _, acting_as) = refs::actor(&ctx.principal)?;
    let reason = reason(&args.reason)?;
    authority::require(ctx, Want::Platform).await?;
    authority::fresh_enough(ctx).await?;
    let target = person_of(ctx, &args.person)?;

    let now = ctx.now();
    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        // `granted_by` is a column of `identity`, which is what every other
        // writer of it puts there and what the operators query resolves a
        // display name through. The requirement's word is "person"; the
        // person is one join away and the id that is *accountable* is the
        // registration that authenticated.
        batch.one(GRANT, bind![target, identity, now]);
        batch.one(
            GRANT_AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                audit::digest_of(args),
                target,
                reason
            ],
        );
        batch.push(audit::object_stmt(ctx.key, object::PARTY, target.get()));
        batch.any(SHORTEN, bind![now + OPERATOR_SESSION_TTL, target]);
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// Take `platform:* #operator` away.
pub async fn revoke_operator<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &RevokeOperator,
) -> Outcome<Committed> {
    authority::not_impersonating(&ctx.principal)?;
    let (identity, _, acting_as) = refs::actor(&ctx.principal)?;
    let reason = reason(&args.reason)?;
    authority::require(ctx, Want::Platform).await?;
    authority::fresh_enough(ctx).await?;
    let target = person_of(ctx, &args.person)?;

    // Read the count so the refusal is one the caller can be given, rather
    // than a guard miss that reads as "the world moved". The guard in `REVOKE`
    // is the same rule where it is transactional.
    if authority::operator_count(&ctx.store.reads()).await? <= 1 {
        return decline();
    }
    // Somebody who is not an operator has nothing to take away, and saying so
    // is not a disclosure: the operators list is an operator-visible query
    // anyway, and the caller is one.
    if !authority::is_operator(&ctx.store.reads(), target).await? {
        return decline();
    }

    let now = ctx.now();
    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(REVOKE, bind![target]);
        batch.one(
            REVOKE_AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                audit::digest_of(args),
                target,
                reason
            ],
        );
        batch.push(audit::object_stmt(ctx.key, object::PARTY, target.get()));
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// The person a delegation names.
fn person_of<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    named: &rn_api::ids::PublicId,
) -> Outcome<Id<Person>> {
    ids::decode::<Person>(ctx.store.ids(), named).map_or_else(|_| decline(), Ok)
}

/// The stated reason, bounded like `RuleMatch`'s evidence.
///
/// [`Invalid`] and not the uniform decline, because it describes the shape of
/// what the caller itself sent and a form has to render it beside the field.
fn reason(raw: &str) -> Outcome<&str> {
    let reason = raw.trim();
    if reason.is_empty() {
        return Err(Invalid::Missing("reason").into());
    }
    if reason.chars().count() > EVIDENCE_LIMIT {
        return Err(Invalid::TooLong {
            field: "reason",
            limit: EVIDENCE_LIMIT,
        }
        .into());
    }
    Ok(reason)
}
