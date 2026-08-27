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

/// One raft group's membership as this node currently sees it.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RaftGroup {
    pub healthy: bool,
    pub voters: usize,
    pub expected_voters: usize,
    pub leader: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct Readiness {
    pub status: &'static str,
    pub node: String,
    pub version: String,
    pub raft: RaftGroups,
}

/// Both groups, separately. A 503 that reported only `voters: 3` and said
/// nothing about the cache raft is exactly the blind spot that made the first
/// formation failure unreadable — the two groups have independent membership.
#[derive(Debug, Serialize)]
pub struct RaftGroups {
    pub sqlite: RaftGroup,
    pub cache: RaftGroup,
}

impl RaftGroups {
    #[must_use]
    pub fn ready(&self) -> bool {
        self.sqlite.ready() && self.cache.ready()
    }
}

impl RaftGroup {
    #[must_use]
    pub fn ready(&self) -> bool {
        self.healthy && self.voters == self.expected_voters
    }
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let expected = db::expected_voters();
    let sqlite = RaftGroup {
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
    };
    let cache = RaftGroup {
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
    };

    let raft = RaftGroups { sqlite, cache };
    let ready = raft.ready();
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
        }),
    )
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
