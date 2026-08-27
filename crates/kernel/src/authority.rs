//! `allows` — the last clause of every command's authorisation.
//!
//! The ruling is that a platform operator has full authority over every party
//! and resource in the deployment. Before this module that was true of eight
//! commands and false of eighteen (`docs/stories/platform-admin-requirements.md`
//! finding **F1**): an operator could revoke a session and could not
//! `set_role`, could transfer a resource and could not `invite`. The gap was
//! not a rule anybody wrote down — it was the absence of one, repeated
//! eighteen times.
//!
//! So there is one function. Every command's authorisation ends in
//! [`allows`], and `platform:* #operator` is its last clause. Not a
//! middleware: the check stays *inside* the command, which is
//! [`crate::cmd`]'s stated rule and the reason a route that forgot to call it
//! would be a hole the type system cannot see. What is shared is the sentence
//! "…or an operator", not the place it is asked.
//!
//! ## The order matters, and it is not an optimisation
//!
//! The ordinary clause is evaluated first and short-circuits. `check()` is the
//! hottest query in the deployment and it is keyed on the subject side; a
//! design that folded the operator relation *into* `check` would add a subject
//! to every expansion and a row to every principal — widening the one query
//! the whole model's latency rests on, for a case that is true for one person
//! in the deployment. The operator relation is therefore a second, separate,
//! indexed read, made only when the ordinary rule already said no.
//!
//! ## Authority is the human's
//!
//! [`allows`] asks about the *person*, never the party a session is acting as
//! ([`crate::cmd::member`]'s middle value). `ActAs` moves attribution; if it
//! moved authority, switching a session would grant something switching it
//! back takes away. The same sentence settles impersonation: an impersonated
//! session's person *is* the target, so it carries the target's authority and
//! not the operator's, and nothing in `check()` learns a new case.

use crate::bind;
use crate::cmd::{Ctx, member};
use crate::error::{Outcome, decline};
use crate::feed::Feed;
use crate::ids::{Id, Person};
use crate::org::{self, MemberRole};
use crate::relation::{self, Object, Relation};
use crate::store::{Count, Reads, Sql};

/// What a command needs to be true before it may act.
///
/// Every variant is the command's *ordinary* rule — the one that holds for
/// everybody. [`allows`] adds the operator clause after it, once, in one
/// place.
#[derive(Debug, Clone, Copy)]
pub enum Want {
    /// The command's own rule, already answered. For the handful whose
    /// ordinary rule is not a role or a relation — `may_share`'s two cases,
    /// the last-owner arithmetic, "this identity is mine" — the command works
    /// its own answer out and hands it here, so that even those end in the
    /// same sentence.
    Settled(bool),
    /// A role in a container: an organization or a group, addressed by its
    /// party row.
    Role {
        /// The container's party id.
        container: Id<Person>,
        /// The role the command needs, covered by anything above it.
        role: MemberRole,
    },
    /// A relation on an object, through `check()`.
    On {
        /// What the command needs.
        relation: Relation,
        /// What it needs it on.
        object: Object,
    },
    /// This person and nobody else — the shape a command about somebody's own
    /// row wants. An operator still passes, which is the whole ruling.
    Themselves(Id<Person>),
    /// Nothing but the operator relation. The rules that are not anybody
    /// else's at all: ruling on a merge, registering a relying party,
    /// delegating operator authority.
    Platform,
}

/// May this principal do `want` — as themselves, or as the operator?
///
/// One extra indexed read in the operator case and none in the ordinary one,
/// because the ordinary clause short-circuits.
pub async fn allows<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, want: Want) -> Outcome<bool> {
    let (_, person, _) = member(&ctx.principal)?;
    if ordinary(ctx, person, want).await? {
        return Ok(true);
    }
    is_operator(&ctx.store.reads(), person).await
}

/// [`allows`], as the refusal a command returns.
///
/// The uniform decline, because "not permitted" and "no such row" are one
/// answer everywhere else in this crate and an authorisation that said which
/// would be the exception that undoes the rest.
pub async fn require<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, want: Want) -> Outcome<()> {
    if allows(ctx, want).await? {
        Ok(())
    } else {
        decline()
    }
}

/// The clause that is not the operator's.
async fn ordinary<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    person: Id<Person>,
    want: Want,
) -> Outcome<bool> {
    match want {
        Want::Settled(held) => Ok(held),
        Want::Role { container, role } => {
            org::holds(&ctx.store.reads(), container, person, role).await
        }
        Want::On { relation, object } => {
            let subjects = crate::principal::expand(&ctx.store.reads(), &ctx.principal).await?;
            relation::check(&ctx.store.reads(), &subjects, relation, object).await
        }
        Want::Themselves(who) => Ok(who == person),
        Want::Platform => Ok(false),
    }
}

/// Whether a person holds `platform:* #operator`.
///
/// Platform administration is a relation, not a column and not a role enum —
/// the kernel report's ruling, and the reason there is no `is_admin` anywhere
/// in this crate. The row is written by the configured bootstrap
/// ([`crate::bootstrap_operator`], which refuses the second one) or by
/// [`crate::cmd::grant_operator`], which has an actor and an audit row.
pub async fn is_operator(store: &impl Reads, person: Id<Person>) -> Outcome<bool> {
    let rows = store.query::<Count>(OPERATOR_SQL, bind![person]).await?;
    Ok(rows.first().is_some_and(|c| c.0 > 0))
}

/// The operator lookup, exposed so the "no full scan" gate can assert that it
/// rides `relation_unique_idx` — it is the last clause of every command in the
/// deployment.
pub const OPERATOR_SQL: &str = "SELECT count(*) AS n FROM relation \
     WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator' \
       AND subject_kind = 'person' AND subject_id = $1";

/// How many operators the deployment has. `RevokeOperator`'s last-owner rule.
pub const OPERATOR_COUNT_SQL: &str = "SELECT count(*) AS n FROM relation \
     WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator'";

/// How many people operate this deployment.
pub async fn operator_count(store: &impl Reads) -> Outcome<i64> {
    Ok(store
        .query::<Count>(OPERATOR_COUNT_SQL, bind![])
        .await?
        .first()
        .map_or(0, |row| row.0))
}
