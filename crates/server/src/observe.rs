//! The observation lane, running.
//!
//! The kernel owns the queues and the statements ([`rn_kernel::observe`]);
//! this is the timer and the subscription that make them happen. Without it
//! `last_seen_at` is written by nobody, the match queue is fed by nobody, and
//! `/platform/cluster` reports a number that only ever goes up — which is what
//! a lane with no drain looks like from the outside.
//!
//! Two inputs, one task:
//!
//! * **the change feed** — `Register`, `VerifyEmail` and `AddFactor` are the
//!   three events that change what an identity has *proven*, so they and
//!   nothing else queue a match scan ([`Matches::observe`]). The cursor starts
//!   at zero rather than at the head: a node that has just booted has scanned
//!   nothing, the scan's only statement is an upsert on the pair, and a
//!   bounded page of history is one indexed range read. A signal missed while
//!   this node was down is therefore caught rather than lost.
//! * **a timer** — every [`INTERVAL`] the queues are drained and the two
//!   sweeps run. Each is bounded by the kernel (`QUEUE_LIMIT`, `SWEEP_LIMIT`),
//!   so a tick is a short write however long the node was busy.
//! * **a second timer** — every [`REPORT_INTERVAL`] this node upserts its own
//!   `node_report` row ([`report`]). It is here rather than on the change feed
//!   because a heartbeat is an observation, not a command: putting one on the
//!   feed would add 5 760 rows a day per node to a table that is also the
//!   audit log and has no retention.
//!
//! Nothing here is authoritative and nothing waits on it, which is the whole
//! reason it is allowed to be late, lossy and off the request path.

use std::time::Duration;

use futures_util::StreamExt;
use rn_kernel::Offset;
use rn_kernel::feed::{Feed, READ_LIMIT};
use rn_kernel::store::Reads as _;
use tokio::task::JoinHandle;

use crate::state::AppState;

/// How often the lane drains.
///
/// Shorter than [`rn_kernel::observe::THROTTLE`], because the throttle is
/// enforced in the statement: ticking more often than the window costs a
/// no-op update rather than a write, and ticking less often would make the
/// throttle the interval instead of the floor.
pub const INTERVAL: Duration = Duration::from_secs(60);

/// How often this node writes down what it is.
///
/// The number `platform-nodes` reads it back against: a row older than three
/// of these renders as *not reporting*
/// (`crate::api::query::platform_nodes::STALE_AFTER`). Three rather than one so
/// a single missed tick — a busy node, a leader election, a slow disk — is not
/// reported as an outage; and fifteen seconds rather than sixty so that a node
/// which really has gone is named within a minute, which is what B4.1's
/// acceptance measures.
pub const REPORT_INTERVAL: Duration = Duration::from_secs(15);

/// This node's row in `node_report`, written by this node and nobody else.
///
/// An upsert on the primary key, so a restart reuses the row rather than
/// growing the table, and a node can only ever overwrite its own reading.
const REPORT_SQL: &str = "INSERT INTO node_report \
     (node_id, node, version, raft_role, feed_head, reported_at) \
     VALUES ($1, $2, $3, $4, $5, $6) \
     ON CONFLICT (node_id) DO UPDATE SET \
       node = excluded.node, version = excluded.version, \
       raft_role = excluded.raft_role, feed_head = excluded.feed_head, \
       reported_at = excluded.reported_at";

/// Which voter this process is, in raft's own numbering.
///
/// The same variable `db::cluster` reads, read the same way: a dev topology is
/// node 1 and says so, and a production node that has not been told is a
/// misconfiguration the database layer refuses long before this runs.
#[must_use]
pub fn node_id() -> i64 {
    std::env::var("RN_SITE_HQL_NODE_ID")
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(1)
}

/// Write down what this node is, once.
///
/// Every field is something this process witnessed: the version it was built
/// at, what its own raft group currently tells it about the leader, and where
/// its feed ends. Nothing is derived from another node, because nothing here
/// can see another node — which is exactly why the row exists.
///
/// Public because it is also how a test asks for a report without waiting
/// fifteen seconds for one.
pub async fn report(state: &AppState) -> bool {
    let groups = crate::ops::raft_groups(state).await;
    let me = node_id();
    // This node's own belief, and named as such: `unknown` is what a group
    // with no leader says, and two nodes both claiming `leader` mid-election
    // is two readings rather than a contradiction worth hiding.
    let role = match groups.sqlite.leader {
        Some(leader) if leader as i64 == me => "leader",
        Some(_) => "follower",
        None => "unknown",
    };
    let head = state.feed.head().await.unwrap_or(0);
    let written = state
        .store
        .execute(
            REPORT_SQL,
            rn_kernel::bind![
                me,
                state.node.as_ref(),
                state.version.as_ref(),
                role,
                i64::try_from(head).unwrap_or(i64::MAX),
                state.store.now()
            ],
        )
        .await;
    match written {
        Ok(_) => true,
        // A node that cannot write its own row is a node the other two will
        // shortly render as *not reporting*, which is the truth. It is worth a
        // line, and it is not worth failing a request over.
        Err(error) => {
            tracing::warn!(%error, "this node could not write its report");
            false
        }
    }
}

/// What one tick wrote.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Drained {
    /// `last_seen_at` rows written.
    pub seen: usize,
    /// `match_candidate` rows proposed or re-scored.
    pub candidates: usize,
    /// Expired sessions deleted.
    pub sessions: usize,
    /// Unclaimable links deleted.
    pub links: usize,
}

/// Drain both queues and run both sweeps, once.
///
/// Public because it is also how a test asks for a tick without waiting for
/// one: the timer calls exactly this.
pub async fn tick(state: &AppState) -> Drained {
    let store = state.store.as_ref();
    Drained {
        seen: count(state.observations.drain(store).await, "last_seen"),
        candidates: count(state.matches.drain(store).await, "match scan"),
        sessions: count(rn_kernel::observe::sweep_sessions(store).await, "sweep"),
        links: count(rn_kernel::observe::sweep_links(store).await, "link sweep"),
    }
}

/// An observation that failed is a stale timestamp, never an error a caller
/// hears about — but it is worth a line, because a lane that is failing every
/// tick is a lane that is not running.
fn count(outcome: rn_kernel::Outcome<usize>, what: &'static str) -> usize {
    match outcome {
        Ok(written) => written,
        Err(error) => {
            tracing::warn!(%error, what, "an observation drain failed");
            0
        }
    }
}

/// A running lane. Dropping it stops the task.
///
/// The handle is what makes "shut down on exit" true rather than hoped for: a
/// detached `tokio::spawn` outlives its state and goes on writing to a
/// database the process is closing.
#[derive(Debug)]
pub struct Lane(Option<JoinHandle<()>>);

impl Lane {
    /// Whether the task is still there.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.0.as_ref().is_some_and(|task| !task.is_finished())
    }

    /// Stop the lane and wait until the task is actually gone.
    ///
    /// What `main` wants on the way out. Dropping the handle asks the task to
    /// stop; this is the half that knows when it has, and a write still in
    /// flight while hiqlite is being handed back is what turns an ordinary
    /// restart into an auto-heal boot.
    pub async fn stop(mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for Lane {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

/// Start the lane on the production interval.
#[must_use]
pub fn spawn(state: AppState) -> Lane {
    spawn_every(state, INTERVAL, REPORT_INTERVAL)
}

/// Start the lane on intervals of the caller's choosing.
///
/// Two timers rather than one, because the two jobs have different reasons to
/// be on a clock: the drains are bounded work that may be late, and the report
/// is a freshness signal whose *period* is what the reader interprets. A
/// report folded into the minute tick would make "not reporting" mean three
/// minutes, which is long enough for an operator to have already asked
/// somebody.
#[must_use]
pub fn spawn_every(state: AppState, interval: Duration, report_interval: Duration) -> Lane {
    Lane(Some(tokio::spawn(async move {
        let mut wakes = Box::pin(state.feed.subscribe());
        let mut timer = tokio::time::interval(interval);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        timer.tick().await;
        let mut reports = tokio::time::interval(report_interval);
        reports.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut cursor: Offset = 0;
        loop {
            tokio::select! {
                Some(offset) = wakes.next() => {
                    cursor = note(&state, cursor, offset).await;
                }
                _ = timer.tick() => {
                    tick(&state).await;
                }
                // Not skipped on the first tick the way the drain is: a node
                // that has just come up is exactly the node whose peers are
                // waiting to hear what version it is running.
                _ = reports.tick() => {
                    report(&state).await;
                }
            }
        }
    })))
}

/// Queue a scan for every identity the feed says has proven something new.
///
/// A wake-up is a hint; what is acted on comes from the durable read, from
/// this task's own cursor, which is what makes the lane resumable rather than
/// dependent on having been listening.
async fn note(state: &AppState, cursor: Offset, offset: Offset) -> Offset {
    if offset <= cursor {
        return cursor;
    }
    let events = match state.feed.read(cursor, READ_LIMIT).await {
        Ok(events) => events,
        Err(error) => {
            tracing::warn!(%error, "the change feed could not be read for observation");
            return cursor;
        }
    };
    let mut moved = cursor;
    for committed in events {
        if state.matches.observe(&committed.event) {
            tracing::trace!(
                offset = committed.offset,
                "an identity was queued for scanning"
            );
        }
        moved = moved.max(committed.offset);
    }
    moved
}
