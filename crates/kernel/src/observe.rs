//! The observation lane: writes nothing authoritative, loses nothing that
//! matters.
//!
//! `last_seen` is the archetype. It is read by a human looking at a device
//! list and by nothing else — no authorisation consults it, no invariant
//! depends on it, and a value five minutes stale is indistinguishable from a
//! fresh one. Writing it on the request path would put a raft commit inside
//! every authenticated read, which is both the slowest thing in the request
//! and the crack through which "read paths never write" stops being true.
//!
//! So observations are enqueued, coalesced by session, and drained on a timer:
//! at most one write per session per [`THROTTLE`], and a bounded queue that
//! drops rather than grows. Sliding session expiry rides the same write, since
//! it is the same row and the same indifference to being a few minutes late.
//!
//! The expiry sweep is here for the same reason: it is maintenance, it is
//! bounded, and nothing waits on it.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::Timestamp;
use crate::bind;
use crate::domain::SESSION_TTL;
use crate::error::Outcome;
use crate::ids::{Id, Session};
use crate::store::{Sql, Store};

/// At most one `last_seen` write per session per five minutes.
pub const THROTTLE: i64 = 300;

/// How many distinct sessions the queue will hold before it starts dropping.
///
/// A bound is not a nicety: without one, a burst of traffic against many
/// sessions turns a lossy convenience into unbounded memory. Dropping is the
/// correct backpressure here — the observation is worthless enough that losing
/// it costs a stale timestamp and nothing else.
pub const QUEUE_LIMIT: usize = 8_192;

/// Pending `last_seen` observations, coalesced by session.
#[derive(Debug, Default)]
pub struct Observations {
    pending: Mutex<HashMap<i64, Pending>>,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    /// The newest sighting.
    at: Timestamp,
    /// Whether the session was close enough to expiry to renew while we are
    /// writing the row anyway.
    renew: bool,
}

impl Observations {
    /// An empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Note that a session was used.
    ///
    /// Returns whether the observation was kept. `false` means the queue was
    /// full, which is a dropped timestamp and never an error.
    pub fn touch(&self, session: Id<Session>, at: Timestamp, renew: bool) -> bool {
        let mut pending = self.lock();
        if let Some(existing) = pending.get_mut(&session.get()) {
            existing.at = existing.at.max(at);
            existing.renew |= renew;
            return true;
        }
        if pending.len() >= QUEUE_LIMIT {
            return false;
        }
        pending.insert(session.get(), Pending { at, renew });
        true
    }

    /// How many sessions are waiting to be written.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing is waiting.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Write everything that is due, and return how many rows were written.
    ///
    /// "Due" is `last_seen_at` older than [`THROTTLE`], checked in the
    /// statement rather than in memory: two processes draining the same
    /// cluster must not both write, and the row is the only thing they share.
    pub async fn drain<S: Sql>(&self, store: &Store<S>) -> Outcome<usize> {
        let batch: Vec<(i64, Pending)> = {
            let mut pending = self.lock();
            pending.drain().collect()
        };
        let now = store.clock().now();
        let mut written = 0;
        for (session, observed) in batch {
            let expires = if observed.renew { now + SESSION_TTL } else { 0 };
            written += store
                .execute(
                    "UPDATE session \
                     SET last_seen_at = $1, expires_at = max(expires_at, $2) \
                     WHERE id = $3 AND last_seen_at <= $4",
                    bind![observed.at, expires, session, observed.at - THROTTLE],
                )
                .await?;
        }
        Ok(written)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<i64, Pending>> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// How many expired sessions one sweep will delete.
///
/// Bounded so the sweep is a short write rather than an unbounded one holding
/// the raft leader while a year of rows goes away. The caller runs it on a
/// timer and it converges.
pub const SWEEP_LIMIT: usize = 1_000;

/// Delete a bounded batch of expired sessions.
///
/// Returns how many went. A caller that gets [`SWEEP_LIMIT`] back should run
/// again rather than wait for the next tick.
pub async fn sweep_sessions<S: Sql>(store: &Store<S>) -> Outcome<usize> {
    let now = store.clock().now();
    Ok(store
        .execute(
            "DELETE FROM session WHERE id IN \
             (SELECT id FROM session WHERE expires_at <= $1 LIMIT $2)",
            bind![now, SWEEP_LIMIT as i64],
        )
        .await?)
}

/// Delete a bounded batch of links that can no longer be claimed.
pub async fn sweep_links<S: Sql>(store: &Store<S>) -> Outcome<usize> {
    let now = store.clock().now();
    Ok(store
        .execute(
            "DELETE FROM link WHERE id IN \
             (SELECT id FROM link WHERE expires_at <= $1 LIMIT $2)",
            bind![now, SWEEP_LIMIT as i64],
        )
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Count, Reads};
    use crate::testing::Local;

    async fn last_seen(harness: &Local) -> i64 {
        harness
            .store()
            .query::<Count>("SELECT last_seen_at AS n FROM session", bind![])
            .await
            .expect("query runs")[0]
            .0
    }

    #[tokio::test]
    async fn a_drain_writes_once_and_then_holds_off_for_the_throttle_window() {
        let harness = Local::new();
        let who = harness
            .register("Ronit", "seen@example.test")
            .await
            .expect("registers");
        let session = who.principal.session().expect("a member has a session");
        let created = last_seen(&harness).await;
        let observations = Observations::new();

        // Inside the window: the statement declines to write, so two processes
        // draining the same cluster cannot both do it.
        harness.advance(10);
        observations.touch(session, harness.store().clock().now(), false);
        assert_eq!(
            observations.drain(harness.store()).await.expect("drains"),
            0
        );
        assert_eq!(last_seen(&harness).await, created);

        // Past it: one row.
        harness.advance(THROTTLE);
        let now = harness.store().clock().now();
        observations.touch(session, now, false);
        assert_eq!(
            observations.drain(harness.store()).await.expect("drains"),
            1
        );
        assert_eq!(last_seen(&harness).await, now);
        assert!(observations.is_empty(), "the queue is emptied by draining");
    }

    #[tokio::test]
    async fn a_renewal_rides_the_same_write_and_never_shortens_a_session() {
        let harness = Local::new();
        let who = harness
            .register("Ronit", "renewed@example.test")
            .await
            .expect("registers");
        let session = who.principal.session().expect("a member has a session");
        async fn expires(h: &Local) -> i64 {
            h.store()
                .query::<Count>("SELECT expires_at AS n FROM session", bind![])
                .await
                .expect("query runs")[0]
                .0
        }
        let before = expires(&harness).await;

        harness.advance(crate::domain::RENEW_AFTER + 1);
        let observations = Observations::new();
        observations.touch(session, harness.store().clock().now(), true);
        assert_eq!(
            observations.drain(harness.store()).await.expect("drains"),
            1
        );
        assert!(expires(&harness).await > before, "the session slid forward");

        // A sighting without a renewal never pulls the expiry back.
        harness.advance(THROTTLE + 1);
        let slid = expires(&harness).await;
        observations.touch(session, harness.store().clock().now(), false);
        observations.drain(harness.store()).await.expect("drains");
        assert_eq!(expires(&harness).await, slid);
    }

    #[tokio::test]
    async fn the_sweeps_are_bounded_and_take_only_what_has_run_out() {
        let harness = Local::new();
        let who = harness
            .register("Ronit", "swept@example.test")
            .await
            .expect("registers");
        let factor: Vec<crate::store::RowId<crate::ids::Factor>> = harness
            .store()
            .query("SELECT id FROM factor WHERE kind = 'email'", bind![])
            .await
            .expect("query runs");
        crate::cmd::mint_verification(harness.store(), factor[0].0)
            .await
            .expect("mints");

        assert_eq!(sweep_sessions(harness.store()).await.expect("sweeps"), 0);
        assert_eq!(sweep_links(harness.store()).await.expect("sweeps"), 0);

        harness.advance(crate::domain::SESSION_TTL + 1);
        assert_eq!(sweep_sessions(harness.store()).await.expect("sweeps"), 1);
        assert_eq!(sweep_links(harness.store()).await.expect("sweeps"), 1);
        assert_eq!(
            crate::principal::resolve(harness.store(), &who.token)
                .await
                .expect("resolves")
                .principal,
            crate::principal::Principal::Anonymous
        );
    }

    #[test]
    fn many_sightings_of_one_session_are_one_pending_write() {
        let observations = Observations::new();
        for at in 1_000..1_100 {
            assert!(observations.touch(Id::new(1), at, false));
        }
        assert_eq!(observations.len(), 1, "coalesced by session");
    }

    #[test]
    fn a_full_queue_drops_rather_than_grows() {
        let observations = Observations::new();
        for n in 0..QUEUE_LIMIT as i64 {
            assert!(observations.touch(Id::new(n + 1), 1_000, false));
        }
        assert!(
            !observations.touch(Id::new(QUEUE_LIMIT as i64 + 1), 1_000, false),
            "the bound is real"
        );
        assert!(
            observations.touch(Id::new(1), 1_001, false),
            "an already-queued session is still coalesced when the queue is full"
        );
        assert_eq!(observations.len(), QUEUE_LIMIT);
    }

    #[test]
    fn a_renewal_anywhere_in_the_window_survives_coalescing() {
        let observations = Observations::new();
        observations.touch(Id::new(1), 1_000, false);
        observations.touch(Id::new(1), 1_001, true);
        observations.touch(Id::new(1), 1_002, false);
        let pending = observations.lock();
        let entry = pending.get(&1).expect("queued");
        assert!(entry.renew, "the renewal is not lost to a later sighting");
        assert_eq!(entry.at, 1_002, "and the newest sighting wins");
    }
}
