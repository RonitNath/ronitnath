//! `GET /api/q/cluster` — what this node is, and what its raft groups say.
//!
//! Not a row set and not subscribable: nothing here comes off the change feed,
//! because none of it is a *change* anybody commanded. It is the node's own
//! account of itself — the two raft groups' membership as this process
//! currently sees them, where the change feed ends, how many subscriptions it
//! is carrying, and how much observation work is queued behind it.
//!
//! Every field is something the process actually witnessed. `voters` is what
//! the membership config lists, not what a configuration file hoped for, which
//! is why `expected_voters` sits beside it: the two disagreeing is the whole
//! signal a half-formed cluster gives.

use serde::{Deserialize, Serialize};

/// One raft group's membership as one node currently sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaftView {
    /// Whether the group answered its health probe.
    pub healthy: bool,
    /// How many voters its membership config lists.
    pub voters: usize,
    /// How many the deployment was provisioned for.
    pub expected_voters: usize,
    /// The node id this process believes is leader, if it believes in one.
    pub leader: Option<u64>,
}

impl RaftView {
    /// A group is ready when it is healthy *and* fully formed. More voters
    /// than configured is a peer map that disagrees with the cluster, which is
    /// a formation problem rather than a bonus.
    #[must_use]
    pub const fn ready(&self) -> bool {
        self.healthy && self.voters == self.expected_voters
    }
}

/// Both groups, separately. They have independent membership, and a readiness
/// answer that reported only one of them is the blind spot that made the first
/// formation failure unreadable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaftViews {
    /// The SQLite state machine: everything the schema holds.
    pub sqlite: RaftView,
    /// The cache group: the warm session lookup.
    pub cache: RaftView,
}

impl RaftViews {
    /// Whether both groups are ready.
    #[must_use]
    pub const fn ready(&self) -> bool {
        self.sqlite.ready() && self.cache.ready()
    }
}

/// What one node says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterView {
    /// Which replica this process is.
    pub node: String,
    /// The revision it is running.
    pub version: String,
    /// Its two raft groups.
    pub raft: RaftViews,
    /// Where the change feed ends — the offset a fresh subscriber is seeded
    /// at, and the `audit` id this node has applied.
    ///
    /// The same number `/readyz` reports under the same name, and the same one
    /// `node_report.feed_head` carries for every node in the formation. One
    /// fact, one spelling: a restore that says which offset it restored to is
    /// checked against this, and a reader who had to learn two names for it
    /// would be a reader who could compare the wrong two numbers.
    pub feed_head: u64,
    /// How many `/api/sub` sockets this node is carrying.
    pub subscribers: usize,
    /// How many `last_seen` observations are queued and not yet written.
    ///
    /// The observation lane is lossy by definition and drained on a timer, so
    /// a number here is work in hand rather than a fault — but a number that
    /// only ever climbs is a drain that is not running, which is the one thing
    /// this counter can witness that nothing else does.
    pub observations_pending: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn group(healthy: bool, voters: usize, expected: usize) -> RaftView {
        RaftView {
            healthy,
            voters,
            expected_voters: expected,
            leader: Some(1),
        }
    }

    #[test]
    fn a_group_is_ready_only_when_it_is_healthy_and_fully_formed() {
        assert!(group(true, 3, 3).ready());
        assert!(!group(false, 3, 3).ready());
        assert!(!group(true, 2, 3).ready());
        assert!(!group(true, 4, 3).ready());
    }

    #[test]
    fn one_unformed_group_holds_the_whole_node_unready() {
        assert!(
            !RaftViews {
                sqlite: group(true, 3, 3),
                cache: group(true, 1, 3),
            }
            .ready()
        );
    }

    #[test]
    fn a_cluster_view_round_trips() {
        let view = ClusterView {
            node: "local".into(),
            version: "dev".into(),
            raft: RaftViews {
                sqlite: group(true, 1, 1),
                cache: group(true, 1, 1),
            },
            feed_head: 40,
            subscribers: 2,
            observations_pending: 0,
        };
        crate::testing::round_trip(&view);
    }
}
