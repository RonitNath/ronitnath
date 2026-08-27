//! `SignInAs` and `EndImpersonation` — wearing somebody else's hat, and
//! taking it off.
//!
//! The kernel's rule is that the server never invents a principal of its own
//! ([`crate::principal`]), and this command does not break it: what it makes
//! is a **real session row** for an active identity of the target, minted by a
//! command with an actor, an audit row and a stated reason. So
//! `principal::expand` resolves the *target's* real subject set, `check()`
//! learns no new case, and the deployment's hottest query is untouched — which
//! is the whole design and the reason C11.2 says a leg that widens `check` has
//! misread it.
//!
//! What is different about the row is four columns and one function.
//!
//! * `impersonated_by_identity_id` is the operator. `impersonation_reason` is
//!   what they said. `expires_at` is thirty minutes, not a fortnight.
//! * [`crate::cmd::refs::actor`] reads the first of those and returns the
//!   operator as `actor_identity_id` and the target as `acting_as`. Every
//!   command already routes its audit row through that function, so *the
//!   operator is the actor and the person is the hat* everywhere in the
//!   deployment, by construction rather than by twenty-six edits.
//!
//! ## The five endings
//!
//! An impersonation is over when any of these happens, and each is somebody
//! else's mechanism rather than a timer this file owns:
//!
//! * [`end_impersonation`] deletes the session. The operator's own cookie is
//!   untouched and is what they go back to.
//! * `IMPERSONATION_TTL` runs out — the ordinary expiry every session has.
//! * the operator stops being one, checked at resolve time for impersonated
//!   sessions only ([`crate::principal::OPERATOR_STILL_SQL`]). It is checked
//!   there because `RevokeOperator` has no way to know which hats somebody is
//!   wearing.
//! * the target is disabled — the guard `SignIn` and `resolve` already carry.
//! * the target signs out everywhere, which is `RevokeSession` on the row.
//!
//! ## Who it may be done to
//!
//! Not another operator. An operator who could become another operator would
//! make `RevokeOperator` meaningless: the revoked one puts on the revoker's
//! hat and grants it back. That refusal is the one rule here that is about
//! *who* rather than about *whether*, and it is checked against the target's
//! person rather than the identity, because the relation is the human's.

use rn_api::commands::{EndImpersonation, SignInAs};

use super::{Applied, Batch, Ctx, Minted, member, run};
use crate::audit::{self, object};
use crate::authority::{self, Want};
use crate::bind;
use crate::domain::{IMPERSONATION_TTL, Token};
use crate::error::{Invalid, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Id, Identity, Person};
use crate::merge::rule::EVIDENCE_LIMIT;
use crate::store::{Cursor, FromRow, Reads, RowError, Sql, Value};

/// An active identity of an active person. The same pair of statuses
/// [`crate::cmd::sign_in`] reads, because an impersonated session must not
/// reach anywhere a real one could not: a disabled person is not somebody an
/// operator gets to be.
///
/// `ORDER BY id` so the choice is deterministic — a person with three
/// registrations is impersonated through the same one every time, which is
/// what makes the audit trail comparable across two of these.
const IDENTITY: &str = "SELECT i.id AS n FROM identity i \
     JOIN party who ON who.id = i.person_id \
     WHERE i.person_id = $1 AND i.status = 'active' AND who.status = 'active' \
     ORDER BY i.id LIMIT 1";

/// The session, with the operator on it. The guard is the pair of statuses
/// again, restated where it is transactional: a disable that commits between
/// the read and here must win.
const SESSION: &str = "INSERT INTO session \
     (identity_id, acting_as, token_hash, expires_at, created_at, last_seen_at, \
      auth_time, impersonated_by_identity_id, impersonation_reason) \
     SELECT $1, $2, $3, $4, $5, $5, $5, $6, $7 \
     WHERE EXISTS (SELECT 1 FROM identity i JOIN party who ON who.id = i.person_id \
                   WHERE i.id = $1 AND i.status = 'active' AND who.status = 'active') \
     RETURNING id";

const SIGN_IN_AS_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'sign-in-as', $2, $3, $4, $5, \
            json_object('event', 'sign-in-as', 'operator', $2, 'person', $3, \
                        'session', $6, 'reason', $7) \
     WHERE changes() > 0";

/// The guard is the impersonation itself: a session that is not one, or one
/// somebody else already ended, matches nothing.
const END: &str = "DELETE FROM session WHERE id = $1 AND impersonated_by_identity_id IS NOT NULL";

const END_AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'end-impersonation', $2, $3, $4, $5, \
            json_object('event', 'end-impersonation', 'operator', $2, 'person', $3, \
                        'session', $6) \
     WHERE changes() > 0";

struct Row(i64);

impl FromRow for Row {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.int("n")?))
    }
}

/// Mint a session that speaks as somebody else.
pub async fn sign_in_as<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &SignInAs) -> Outcome<Minted> {
    authority::not_impersonating(&ctx.principal)?;
    // The deployment may say no, whatever the operator holds (C11.3). Asked
    // before anything else, so a deployment with it off does not even read
    // whether the target exists.
    if !ctx.impersonation {
        return decline();
    }
    let (operator, _, _) = member(&ctx.principal)?;
    let reason = reason(&args.reason)?;
    authority::require(ctx, Want::Platform).await?;
    authority::fresh_enough(ctx).await?;

    let Ok(target) = ids::decode::<Person>(ctx.store.ids(), &args.person) else {
        return decline();
    };
    // Never another operator. Same answer as a person who does not exist:
    // which operators there are is a question `/platform/operators` answers to
    // somebody who asked it directly.
    if authority::is_operator(&ctx.store.reads(), target).await? {
        return decline();
    }
    let Some(Row(identity)) = ctx.store.query_opt::<Row>(IDENTITY, bind![target]).await? else {
        // No active identity, or a disabled person. One answer for both.
        return decline();
    };
    let identity: Id<Identity> = Id::new(identity);

    let token = Token::mint();
    let now = ctx.now();

    let Applied { committed, fresh } = run(ctx, args, async || {
        let mut batch = Batch::new();
        let session = batch.one(
            SESSION,
            vec![
                Value::from(identity),
                Value::from(target),
                Value::from(token.digest()),
                Value::from(now + IMPERSONATION_TTL),
                Value::from(now),
                Value::from(operator),
                Value::from(reason),
            ],
        );
        batch.one(
            SIGN_IN_AS_AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(operator),
                Value::from(target),
                Value::from(now),
                Value::from(audit::digest_of(args)),
                session.column("id"),
                Value::from(reason),
            ],
        );
        // Two objects: the person it was done to — which is how their own
        // `about-me` finds it — and the session it minted, which is how the
        // operators screen finds the hat that is still on.
        batch.push(audit::object_stmt(ctx.key, object::PARTY, target.get()));
        Ok(batch)
    })
    .await?;

    Ok(Minted {
        committed,
        token: fresh.then_some(token),
    })
}

/// Take the hat off.
///
/// Run from the impersonated session, which is the one being deleted. The
/// operator's own cookie was never touched and is still live — that is what
/// they return to, and it is why this is not a spelling of `SignOut`.
pub async fn end_impersonation<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &EndImpersonation,
) -> Outcome<Committed> {
    let (_, person, session) = member(&ctx.principal)?;
    let Some(operator) = ctx.principal.impersonated_by() else {
        // An ordinary session has no hat to take off, and saying so is not a
        // disclosure — the caller is the session.
        return decline();
    };
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(END, bind![session]);
        batch.one(
            END_AUDIT,
            bind![
                ctx.key.to_string(),
                operator,
                person,
                now,
                audit::digest_of(args),
                session
            ],
        );
        batch.push(audit::object_stmt(ctx.key, object::PARTY, person.get()));
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// The stated reason, bounded like `RuleMatch`'s evidence.
///
/// It lands on the session row as well as the audit row, and the second copy
/// is the point: the person it was done to reads it through `about-me`, and a
/// reason that lived only where operators look would be an explanation nobody
/// owed anybody.
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
