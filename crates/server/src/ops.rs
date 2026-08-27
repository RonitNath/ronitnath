//! Liveness, readiness and identity — the three routes an operator and the
//! edge poll, and the only ones that exist before the application does.
//!
//! There is no `/metrics`: `docs/rebuild/plan.md` lists it as a non-goal for
//! this cut, and a route that reports nothing anyone reads is worse than none.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::db;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/version", get(version))
}

/// Liveness. No dependency, no allocation of consequence, no reason to fail
/// while the process is up. The edge polls this twice a second and Compose
/// restarts the container on it, so it must never answer for the database —
/// that is readiness's job, and restarting on it would turn a leader election
/// into a restart loop.
async fn healthz() -> &'static str {
    "ok"
}

async fn version(State(state): State<AppState>) -> String {
    state.version.to_string()
}

/// One raft group's membership, and both groups together.
///
/// The shapes are `rn_api::cluster`'s: `/readyz` and the operator console's
/// `GET /api/q/cluster` report the same two groups, so they report them in the
/// same words. This module owns the *probe* ([`raft_groups`]); the vocabulary
/// is on the wire, where both readers of it can see it.
pub use rn_api::cluster::{RaftView as RaftGroup, RaftViews as RaftGroups};

#[derive(Debug, Serialize)]
pub struct Readiness {
    pub status: &'static str,
    pub node: String,
    pub version: String,
    pub raft: RaftGroups,
    /// Where this node's change feed ends — the `audit` id it has applied.
    ///
    /// The same field, the same name and the same meaning as
    /// [`rn_api::ClusterView::feed_head`] and as `node_report.feed_head`: one
    /// number about one node, reported by every surface that reports on that
    /// node at all. It is here because a restore is *proven by the health
    /// screen* (F16.3) — `rn-site admin restore` names the offset it restored
    /// to, and the only way to check that claim from outside the process is a
    /// route that says where the feed now ends. `/admin/readyz` on the
    /// loopback listener answers with the same field for the same reason.
    ///
    /// A feed that cannot answer reads as zero, which is what a subscriber
    /// seeding from scratch would see, and is never a reason to fail
    /// readiness: a node whose raft groups are formed is ready whatever its
    /// feed says.
    pub feed_head: u64,
}

/// Ask both raft groups where they stand.
///
/// One probe, two readers: readiness turns it into a status code, and the
/// platform console renders it. A second implementation of this would be a
/// second opinion about the same cluster.
pub async fn raft_groups(state: &AppState) -> RaftGroups {
    let expected = db::expected_voters();
    RaftGroups {
        sqlite: RaftGroup {
            healthy: state.db.is_healthy_db().await.is_ok(),
            voters: state
                .db
                .metrics_db()
                .await
                .map(|metrics| metrics.membership_config.voter_ids().count())
                .unwrap_or(0),
            expected_voters: expected,
            leader: state
                .db
                .metrics_db()
                .await
                .ok()
                .and_then(|metrics| metrics.current_leader),
        },
        cache: RaftGroup {
            healthy: state.db.is_healthy_cache().await.is_ok(),
            voters: state
                .db
                .metrics_cache()
                .await
                .map(|metrics| metrics.membership_config.voter_ids().count())
                .unwrap_or(0),
            expected_voters: expected,
            leader: state
                .db
                .metrics_cache()
                .await
                .ok()
                .and_then(|metrics| metrics.current_leader),
        },
    }
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let raft = raft_groups(&state).await;
    let ready = raft.ready();
    let feed_head = feed_head(&state).await;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(Readiness {
            status: if ready { "ok" } else { "unavailable" },
            node: state.node.to_string(),
            version: state.version.to_string(),
            raft,
            feed_head,
        }),
    )
}

/// Where the feed ends, off the feed rather than off a `SELECT` of this
/// module's own — a reader with its own statement would go on reading a table
/// a later transport no longer owns.
async fn feed_head(state: &AppState) -> u64 {
    use rn_kernel::feed::Feed as _;
    state.feed.head().await.unwrap_or_else(|error| {
        tracing::warn!(%error, "the change feed head could not be read for readiness");
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(healthy: bool, voters: usize, expected: usize) -> RaftGroup {
        RaftGroup {
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
        // More voters than configured is a peer map that disagrees with the
        // cluster, which is a formation problem, not a bonus.
        assert!(!group(true, 4, 3).ready());
    }

    #[test]
    fn one_unformed_group_holds_the_whole_node_unready() {
        let groups = RaftGroups {
            sqlite: group(true, 3, 3),
            cache: group(true, 1, 3),
        };
        assert!(!groups.ready());
    }
}
