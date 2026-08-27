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
//!
//! Nothing here is authoritative and nothing waits on it, which is the whole
//! reason it is allowed to be late, lossy and off the request path.

use std::time::Duration;

use futures_util::StreamExt;
use rn_kernel::Offset;
use rn_kernel::feed::{Feed, READ_LIMIT};
use tokio::task::JoinHandle;

use crate::state::AppState;

/// How often the lane drains.
///
/// Shorter than [`rn_kernel::observe::THROTTLE`], because the throttle is
/// enforced in the statement: ticking more often than the window costs a
/// no-op update rather than a write, and ticking less often would make the
/// throttle the interval instead of the floor.
pub const INTERVAL: Duration = Duration::from_secs(60);

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
    spawn_every(state, INTERVAL)
}

/// Start the lane on an interval of the caller's choosing.
#[must_use]
pub fn spawn_every(state: AppState, interval: Duration) -> Lane {
    Lane(Some(tokio::spawn(async move {
        let mut wakes = Box::pin(state.feed.subscribe());
        let mut timer = tokio::time::interval(interval);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        timer.tick().await;
        let mut cursor: Offset = 0;
        loop {
            tokio::select! {
                Some(offset) = wakes.next() => {
                    cursor = note(&state, cursor, offset).await;
                }
                _ = timer.tick() => {
                    tick(&state).await;
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
