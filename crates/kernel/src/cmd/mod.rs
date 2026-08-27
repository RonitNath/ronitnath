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

mod act_as;
mod add_factor;
mod claim_link;
mod create_document;
mod create_group;
mod create_organization;
mod disable;
mod edit_document;
mod enable;
mod invite;
mod leave;
pub(crate) mod oidc;
mod publish_document;
pub(crate) mod refs;
mod register;
mod remove_factor;
mod remove_member;
mod revoke;
mod revoke_link;
mod revoke_session;
mod set_role;
mod share;
mod sign_in;
mod sign_out;
#[cfg(test)]
mod tests;
mod transfer;
mod verify_email;

pub use act_as::act_as;
pub use add_factor::add_factor;
pub use claim_link::claim_link;
pub use create_document::create_document;
pub use create_group::create_group;
pub use create_organization::create_organization;
pub use disable::disable;
pub use edit_document::edit_document;
pub use enable::enable;
pub use invite::invite;
pub use leave::leave;
pub use oidc::{
    Authorized, Granted, Issued, Registered, authorize, client_credentials, delete_client,
    end_session, exchange_code, refresh_token, register_client, revoke_consent, revoke_token,
    rotate_client_secret, rotate_signing_key, set_handle, update_client,
};
pub use publish_document::publish_document;
pub use register::register;
pub use remove_factor::remove_factor;
pub use remove_member::remove_member;
pub use revoke::revoke;
pub use revoke_link::revoke_link;
pub use revoke_session::revoke_session;
pub use set_role::set_role;
pub use share::{SHARER, share};
pub use sign_in::{LOOKUP as SIGN_IN_SQL, SignedIn, sign_in};
pub use sign_out::sign_out;
pub use transfer::transfer;
pub use verify_email::{mint_verification, verify_email};

// The four merge commands live in `crate::merge`, beside the five steps they
// share and the queue they act on; they are re-exported here so every command
// route is reachable by one name.
pub use crate::merge::{confirm_match, propose_match, rule_match, split};

/// Whether a person holds `platform:* #operator`.
///
/// Re-exported under the name the tier checks outside this crate ask by. It is
/// deliberately *not* how a command authorises itself any more: the eight
/// open-coded calls that used to sit inside command modules are one clause of
/// [`crate::authority::allows`] now, so that the eighteen commands that had no
/// operator path could stop being eighteen separate omissions. What is left
/// here is the read the server makes to decide whether to serve the
/// `/platform` shell at all, which is a question about a bundle rather than
/// about a write.
pub use crate::authority::is_operator as is_platform_operator;

use std::future::Future;

use serde::Serialize;
use uuid::Uuid;

use crate::audit;
use crate::error::{KernelError, Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{Id, Identity, Person, Session};
use crate::principal::Principal;
use crate::store::{Sql, Stmt, Store, StoreError, Value};

/// Everything a command needs that is not its own arguments.
pub struct Ctx<'a, S: Sql, F: Feed> {
    /// The database.
    pub store: &'a Store<S>,
    /// The change feed, notified after the commit.
    pub feed: &'a F,
    /// This deployment's issuer and signing-key seal — what the OpenID
    /// Provider's commands need and no other command reads.
    pub provider: &'a crate::oidc::Provider,
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
///
/// ## Why the plan is `Fn() -> impl Future + Send` and not `AsyncFn`
///
/// `AsyncFn` is the shorter spelling and it cannot say this: there is no
/// stable way to bound the future an `AsyncFn` returns, so a caller awaiting a
/// command inside a `Send` task fails with "implementation of `Send` is not
/// general enough" — the higher-ranked lifetime the sugar hides. Spelling the
/// future out as its own type parameter is what lets `Send` be written down,
/// and it is what makes a command awaitable directly inside an axum handler
/// rather than driven to completion on a blocking thread to dodge the
/// question. Every existing plan closure — `async || { … }` — satisfies this
/// unchanged.
pub async fn run<S, F, A, P, Plan>(ctx: &Ctx<'_, S, F>, args: &A, plan: P) -> Outcome<Applied>
where
    S: Sql,
    F: Feed,
    A: Serialize,
    P: Fn() -> Plan,
    Plan: Future<Output = Outcome<Batch>> + Send,
{
    let started = std::time::Instant::now();
    let digest = audit::digest_of(args);
    if let Some(committed) = audit::replay(ctx.store, ctx.key, &digest).await? {
        return Ok(Applied {
            committed,
            fresh: false,
        });
    }
    let replayed = started.elapsed();

    let mut planning = std::time::Duration::ZERO;
    let mut attempt = 0;
    loop {
        let phase = std::time::Instant::now();
        let batch = plan().await?;
        planning += phase.elapsed();
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

    let commit_ended = started.elapsed();
    let committed = read_back(ctx, &digest).await?;
    let read_back_ended = started.elapsed();
    ctx.feed.notify(committed.offset).await;
    let done = started.elapsed();

    // The decomposition of a command's wall time, on the node that ran it.
    // Off by default and free when it is off; `RUST_LOG=rn_kernel::cmd=debug`
    // is what turns a commit-latency number into an account of where it went
    // (`docs/perf/2026-08-27-f5.md`). Microseconds, because the budget is
    // "one intra-zone RTT plus a millisecond".
    tracing::debug!(
        replay_us = replayed.as_micros() as u64,
        plan_us = planning.as_micros() as u64,
        commit_us = (commit_ended - replayed - planning).as_micros() as u64,
        read_back_us = (read_back_ended - commit_ended).as_micros() as u64,
        notify_us = (done - read_back_ended).as_micros() as u64,
        total_us = done.as_micros() as u64,
        attempts = attempt + 1,
        "command phases"
    );
    Ok(Applied {
        committed,
        fresh: true,
    })
}

/// How long a node will wait to see a batch it has already committed.
const READ_BACK_PATIENCE: std::time::Duration = std::time::Duration::from_secs(2);

/// How often it looks. A follower applies an entry the leader has committed in
/// well under a millisecond on a healthy cluster; this is several looks per
/// that.
const READ_BACK_POLL: std::time::Duration = std::time::Duration::from_micros(200);

/// Read back the audit row of a batch that has just committed.
///
/// The commit went through raft and returned, which means a quorum has it —
/// and says nothing about whether *this* node has applied it yet. Read
/// straight back, a command run on a follower asks its own state machine
/// about a row that is in the log and not yet in the database, and the answer
/// is "committed batch left no audit row": an invariant failure reported for a
/// command that worked perfectly. Measured at 20-46% of `create-document`s
/// posted to a follower (finding 2, `docs/perf/2026-08-27.md`) — which the
/// report attributed to the precondition read, and this is the half of it
/// that was actually being seen.
///
/// So it waits. The row is not in doubt: the batch committed, the audit
/// statement is inside it, and every one of those is guarded by the same
/// guard. What is in doubt is only when this node catches up, and if it never
/// does the invariant failure is the honest answer after all.
async fn read_back<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    digest: &str,
) -> Outcome<crate::event::Committed> {
    let deadline = std::time::Instant::now() + READ_BACK_PATIENCE;
    loop {
        if let Some(committed) = audit::replay(ctx.store, ctx.key, digest).await? {
            return Ok(committed);
        }
        if std::time::Instant::now() >= deadline {
            return Err(KernelError::Invariant(
                "committed batch left no audit row".to_owned(),
            ));
        }
        tokio::time::sleep(READ_BACK_POLL).await;
    }
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
///
/// The middle value is the *person*, not the party the session is acting as.
/// Authority is the human's: `ActAs` moves attribution, and a command that
/// authorised itself against the acted-as party would let switching a session
/// grant something switching it back takes away. Where a command records the
/// acted-as party — its audit row — it asks [`refs::actor`] instead.
pub(crate) fn member(principal: &Principal) -> Outcome<(Id<Identity>, Id<Person>, Id<Session>)> {
    match principal {
        Principal::Member {
            identity,
            person,
            acting_as,
            session,
        } => Ok((*identity, person.unwrap_or(*acting_as), *session)),
        Principal::Bearer { .. } | Principal::Anonymous => decline(),
    }
}
