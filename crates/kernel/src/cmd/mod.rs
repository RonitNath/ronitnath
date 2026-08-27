//! The commands. One module each, one transaction each.
//!
//! Every command has the same shape, and the shape is enforced by [`run`]
//! rather than repeated nine times:
//!
//! 1. **Validate** the arguments. Says what is wrong, because the caller sent
//!    them.
//! 2. **Authorise**, inside the command. Not in a middleware, not in the
//!    router: the check that a person may revoke that session is part of what
//!    revoking a session *means*, and a route that forgot to call it would be
//!    a hole the type system cannot see.
//! 3. **Read** what the decision needs.
//! 4. **Commit** one guarded batch — the audit row and the mutations together,
//!    every statement carrying the same guard.
//! 5. **Read back** the audit row: its id is the offset and its payload is the
//!    event. Fresh commits and idempotent replays go through exactly this
//!    step, so they cannot answer differently.
//! 6. **Notify** the feed, after the commit, never inside it.
//!
//! Step 4 can fail because the world moved between (3) and (4) — every
//! statement affected zero rows, so nothing was written. [`run`] re-reads and
//! tries once more; a second miss is a decline, because a third attempt would
//! be a caller with a stale view arguing with one that is current.

mod add_factor;
mod disable;
mod enable;
mod register;
mod remove_factor;
mod revoke_session;
mod sign_in;
mod sign_out;
#[cfg(test)]
mod tests;
mod verify_email;

pub use add_factor::add_factor;
pub use disable::disable;
pub use enable::enable;
pub use register::register;
pub use remove_factor::remove_factor;
pub use revoke_session::revoke_session;
pub use sign_in::{SignedIn, sign_in};
pub use sign_out::sign_out;
pub use verify_email::{mint_verification, verify_email};

use serde::Serialize;
use uuid::Uuid;

use crate::audit;
use crate::bind;
use crate::error::{KernelError, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, Identity, Person, Session};
use crate::principal::Principal;
use crate::store::{Count, Reads, Sql, Stmt, Store, StoreError, Value};

/// Everything a command needs that is not its own arguments.
pub struct Ctx<'a, S: Sql, F: Feed> {
    /// The database.
    pub store: &'a Store<S>,
    /// The change feed, notified after the commit.
    pub feed: &'a F,
    /// Who is asking.
    pub principal: Principal,
    /// The caller's idempotency key.
    pub key: Uuid,
}

impl<S: Sql, F: Feed> Ctx<'_, S, F> {
    /// The current unix second, taken once per command.
    pub fn now(&self) -> crate::Timestamp {
        self.store.clock().now()
    }
}

/// A reference to a statement already in a batch.
#[derive(Debug, Clone, Copy)]
pub struct StmtRef(usize);

impl StmtRef {
    /// A column of the first row this statement returns, as a parameter for a
    /// later statement in the same batch. The statement must have a
    /// `RETURNING` clause naming the column.
    pub const fn column(self, name: &'static str) -> Value {
        Value::of_stmt(self.0, name)
    }
}

/// A transaction batch under construction.
///
/// The builder hands back a [`StmtRef`] for each statement it takes, so a
/// later statement names an earlier one's output by reference rather than by a
/// hand-counted index that a reordering would silently break.
#[derive(Debug, Default)]
pub struct Batch {
    stmts: Vec<Stmt>,
}

impl Batch {
    /// An empty batch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a statement that must affect exactly one row.
    pub fn one(&mut self, sql: &'static str, params: Vec<Value>) -> StmtRef {
        self.push(Stmt::one(sql, params))
    }

    /// Add a statement whose affected count carries no information.
    pub fn any(&mut self, sql: &'static str, params: Vec<Value>) -> StmtRef {
        self.push(Stmt::any(sql, params))
    }

    /// Add a prepared statement.
    pub fn push(&mut self, stmt: Stmt) -> StmtRef {
        self.stmts.push(stmt);
        StmtRef(self.stmts.len() - 1)
    }

    /// The statements, in order.
    pub fn into_stmts(self) -> Vec<Stmt> {
        self.stmts
    }
}

/// Run a command: replay, plan, commit, read back, notify.
///
/// `plan` is called at most twice, and only ever a second time when the first
/// batch wrote nothing at all.
pub async fn run<S, F, A, P>(ctx: &Ctx<'_, S, F>, args: &A, plan: P) -> Outcome<Applied>
where
    S: Sql,
    F: Feed,
    A: Serialize,
    P: AsyncFn() -> Outcome<Batch>,
{
    let digest = audit::digest_of(args);
    if let Some(committed) = audit::replay(ctx.store, ctx.key, &digest).await? {
        return Ok(Applied {
            committed,
            fresh: false,
        });
    }

    let mut attempt = 0;
    loop {
        let batch = plan().await?;
        match ctx.store.commit(batch.into_stmts()).await {
            Ok(_) => break,
            Err(StoreError::GuardMissed) if attempt == 0 => attempt += 1,
            // The world moved twice. A third read would be arguing with it.
            Err(StoreError::GuardMissed) => return decline(),
            // A UNIQUE violation is either this key arriving twice at once —
            // in which case the winner's row is the answer — or a duplicate
            // the command already checked for and lost the race on, which is
            // an ordinary refusal.
            Err(StoreError::Constraint(_)) => {
                return match audit::replay(ctx.store, ctx.key, &digest).await? {
                    Some(committed) => Ok(Applied {
                        committed,
                        fresh: false,
                    }),
                    None => decline(),
                };
            }
            Err(err) => return Err(KernelError::Store(err)),
        }
    }

    let committed = audit::replay(ctx.store, ctx.key, &digest)
        .await?
        .ok_or_else(|| KernelError::Invariant("committed batch left no audit row".to_owned()))?;
    ctx.feed.notify(committed.offset).await;
    Ok(Applied {
        committed,
        fresh: true,
    })
}

/// What [`run`] produced, and whether this call is what produced it.
#[derive(Debug)]
pub struct Applied {
    /// The event and its offset.
    pub committed: Committed,
    /// `false` when an earlier call with the same key did the work. A command
    /// that mints a secret uses this to decide whether it has one to hand
    /// back.
    pub fresh: bool,
}

/// A command that mints a bearer secret.
///
/// The token is `None` on a replay, and that is not an oversight. A session
/// token exists in the clear exactly once, in the reply that created it; the
/// row keeps only its SHA-256. Reproducing it on a replay would mean storing
/// it, which is the one thing that makes a database read enough to impersonate
/// somebody. A caller that lost the reply signs in again.
#[derive(Debug)]
pub struct Minted {
    /// What was committed.
    pub committed: Committed,
    /// The secret, if this call is the one that minted it.
    pub token: Option<crate::domain::Token>,
}

/// The signed-in principal, or a decline.
///
/// Every command that acts on "my own" anything starts here, and the decline
/// is the same one an unauthorised member gets — being signed out and being
/// unauthorised are not distinguishable to a caller.
pub(crate) fn member(principal: &Principal) -> Outcome<(Id<Identity>, Id<Person>, Id<Session>)> {
    match principal {
        Principal::Member {
            identity,
            acting_as,
            session,
            ..
        } => Ok((*identity, *acting_as, *session)),
        Principal::Bearer { .. } | Principal::Anonymous => decline(),
    }
}

/// Whether a person holds `platform:* #operator`.
///
/// Platform administration is a relation, not a column and not a role enum —
/// the kernel report's ruling, and the reason there is no `is_admin` anywhere
/// in this crate. The row is written by an operator seeding it or by K2's
/// `SetRole`; K1 only reads it, which is why the operator paths here are
/// reachable and testable before that command exists.
pub(crate) async fn is_platform_operator(store: &impl Reads, person: Id<Person>) -> Outcome<bool> {
    let rows = store
        .query::<Count>(
            "SELECT count(*) AS n FROM relation \
             WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator' \
               AND subject_kind = 'person' AND subject_id = $1",
            bind![person],
        )
        .await?;
    Ok(rows.first().is_some_and(|c| c.0 > 0))
}
