//! Match scanning, on the observation lane.
//!
//! `Register`, `VerifyEmail` and `AddFactor` are the three commands that
//! change what an identity has *proven*, which is the only input a signal
//! reads. So the drain is fed by those events and nothing else: scanning on
//! every command would mean re-asking the same question after a sign-in, and
//! scanning on a timer would mean asking it about everybody forever.
//!
//! The queue has the same three properties as `last_seen`'s, for the same
//! reason — it writes nothing an authorisation reads:
//!
//! * **coalesced** — one entry per identity however many events arrive;
//! * **bounded** — a burst drops rather than grows, and a dropped scan costs a
//!   question that the identity's next verification asks again;
//! * **idempotent** — the scan's only statement is an upsert on the pair, so
//!   draining twice writes the same rows twice and means the same thing.

use std::collections::BTreeSet;
use std::sync::Mutex;

use super::QUEUE_LIMIT;
use crate::error::Outcome;
use crate::event::Event;
use crate::ids::{Id, Identity};
use crate::merge::scan_identity;
use crate::store::{Sql, Store};

/// Identities waiting to be scanned for signals.
#[derive(Debug, Default)]
pub struct Matches {
    pending: Mutex<BTreeSet<i64>>,
}

impl Matches {
    /// An empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue an identity. Returns whether it was kept.
    pub fn note(&self, identity: Id<Identity>) -> bool {
        let mut pending = self.lock();
        if pending.contains(&identity.get()) {
            return true;
        }
        if pending.len() >= QUEUE_LIMIT {
            return false;
        }
        pending.insert(identity.get());
        true
    }

    /// Queue whatever an event says has changed about an identity's proofs.
    ///
    /// Returns whether the event was one of the three. A sign-in is not: it
    /// proves something the identity had already proven.
    pub fn observe(&self, event: &Event) -> bool {
        let identity = match event {
            Event::Registered { identity, .. }
            | Event::EmailVerified { identity, .. }
            | Event::FactorAdded { identity, .. } => *identity,
            _ => return false,
        };
        self.note(identity)
    }

    /// How many identities are waiting.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing is waiting.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Scan everything queued, and return how many candidate rows resulted.
    pub async fn drain<S: Sql>(&self, store: &Store<S>) -> Outcome<usize> {
        let batch: Vec<i64> = {
            let mut pending = self.lock();
            std::mem::take(&mut *pending).into_iter().collect()
        };
        let mut written = 0;
        for identity in batch {
            written += scan_identity(store, Id::new(identity)).await?;
        }
        Ok(written)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeSet<i64>> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FactorKind;
    use crate::ids::{Factor, Person, Session};

    #[test]
    fn only_the_events_that_change_a_proof_queue_a_scan() {
        let queue = Matches::new();
        assert!(queue.observe(&Event::Registered {
            identity: Id::new(1),
            person: Id::new(1),
            session: Id::new(1),
        }));
        assert!(queue.observe(&Event::EmailVerified {
            identity: Id::new(2),
            factor: Id::<Factor>::new(1),
        }));
        assert!(queue.observe(&Event::FactorAdded {
            identity: Id::new(3),
            factor: Id::<Factor>::new(2),
            kind: FactorKind::Email,
        }));
        assert!(
            !queue.observe(&Event::SignedIn {
                identity: Id::new(4),
                session: Id::<Session>::new(2),
            }),
            "signing in proves nothing new about who somebody is"
        );
        assert!(!queue.observe(&Event::PartyDisabled {
            party: Id::<Person>::new(1),
        }));
        assert_eq!(queue.len(), 3);
    }

    #[test]
    fn many_events_about_one_identity_are_one_pending_scan() {
        let queue = Matches::new();
        for _ in 0..50 {
            assert!(queue.note(Id::new(7)));
        }
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn a_full_queue_drops_rather_than_grows() {
        let queue = Matches::new();
        for n in 0..QUEUE_LIMIT as i64 {
            assert!(queue.note(Id::new(n + 1)));
        }
        assert!(
            !queue.note(Id::new(QUEUE_LIMIT as i64 + 1)),
            "the bound is real"
        );
        assert!(
            queue.note(Id::new(1)),
            "an already-queued identity still coalesces"
        );
        assert_eq!(queue.len(), QUEUE_LIMIT);
    }
}
