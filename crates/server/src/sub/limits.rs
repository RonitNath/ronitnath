//! How many sockets one person, and one node, may hold open.
//!
//! A subscription costs a task, a feed subscriber and a queue, and nothing
//! about the browser side stops a page opening them in a loop. Two admission
//! limits therefore apply before the upgrade: one per identity, so a single
//! account cannot exhaust the node, and one for the node, so every account
//! together cannot either. Over either limit the socket is closed with `1013`
//! — *try again later* — which is the one close code that means "not you, not
//! now" rather than "never".
//!
//! The count lives in [`Connections`] rather than in each task, because a cap
//! each connection believed privately would not be a cap.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rn_kernel::ids::{Id, Identity};

/// How many live subscriptions one identity may hold.
pub const PER_IDENTITY: usize = 16;

/// How many this node will carry at once, over every identity.
pub const PER_NODE: usize = 4_096;

/// The live subscription count, shared by every socket on the node.
#[derive(Debug)]
pub struct Connections {
    inner: Mutex<Counts>,
    per_identity: usize,
    per_node: usize,
}

#[derive(Debug, Default)]
struct Counts {
    by_identity: HashMap<i64, usize>,
    total: usize,
}

impl Default for Connections {
    fn default() -> Self {
        Self::new(PER_IDENTITY, PER_NODE)
    }
}

/// Why a socket was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// This identity already holds its share.
    Identity,
    /// The node is full.
    Node,
}

impl Connections {
    /// A registry with the caps spelled out, for a test that wants small ones.
    #[must_use]
    pub fn new(per_identity: usize, per_node: usize) -> Self {
        Self {
            inner: Mutex::new(Counts::default()),
            per_identity: per_identity.max(1),
            per_node: per_node.max(1),
        }
    }

    /// Admit a socket, or refuse it. The returned guard releases the slot when
    /// it drops, which is what makes a panicking task give its slot back.
    pub fn admit(self: &Arc<Self>, identity: Id<Identity>) -> Result<ConnectionGuard, Refused> {
        let mut counts = self.lock();
        if counts.total >= self.per_node {
            return Err(Refused::Node);
        }
        let held = counts.by_identity.entry(identity.get()).or_insert(0);
        if *held >= self.per_identity {
            return Err(Refused::Identity);
        }
        *held += 1;
        counts.total += 1;
        Ok(ConnectionGuard {
            connections: Arc::clone(self),
            identity: identity.get(),
        })
    }

    /// How many sockets are live on this node.
    #[must_use]
    pub fn live(&self) -> usize {
        self.lock().total
    }

    fn release(&self, identity: i64) {
        let mut counts = self.lock();
        if let Some(held) = counts.by_identity.get_mut(&identity) {
            *held -= 1;
            if *held == 0 {
                counts.by_identity.remove(&identity);
            }
        }
        counts.total = counts.total.saturating_sub(1);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Counts> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// One admitted socket's slot.
#[derive(Debug)]
pub struct ConnectionGuard {
    connections: Arc<Connections>,
    identity: i64,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.connections.release(self.identity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(raw: i64) -> Id<Identity> {
        Id::new(raw)
    }

    #[test]
    fn one_identity_cannot_take_more_than_its_share() {
        let connections = Arc::new(Connections::new(2, 100));
        let first = connections
            .admit(identity(1))
            .expect("the first is admitted");
        let _second = connections.admit(identity(1)).expect("so is the second");
        assert!(matches!(
            connections.admit(identity(1)),
            Err(Refused::Identity)
        ));
        // And somebody else is unaffected by it.
        let _other = connections
            .admit(identity(2))
            .expect("a different identity");
        assert_eq!(connections.live(), 3);

        drop(first);
        assert_eq!(connections.live(), 2);
        connections
            .admit(identity(1))
            .expect("the freed slot is reusable");
    }

    #[test]
    fn the_node_limit_binds_even_when_no_single_identity_is_over() {
        let connections = Arc::new(Connections::new(10, 2));
        let _a = connections.admit(identity(1)).expect("one");
        let _b = connections.admit(identity(2)).expect("two");
        assert!(matches!(connections.admit(identity(3)), Err(Refused::Node)));
    }

    #[test]
    fn releasing_forgets_the_identity_rather_than_keeping_a_zero() {
        let connections = Arc::new(Connections::new(4, 4));
        {
            let _held = connections.admit(identity(9)).expect("admitted");
            assert_eq!(connections.lock().by_identity.len(), 1);
        }
        assert_eq!(connections.lock().by_identity.len(), 0);
        assert_eq!(connections.live(), 0);
    }
}
