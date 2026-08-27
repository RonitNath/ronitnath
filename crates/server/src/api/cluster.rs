//! `GET /api/q/cluster` — what this node is, asked of the node itself.
//!
//! A query by URL and not by shape: it has no rows, no keys and no diff,
//! because nothing it reports is a *change* anybody commanded. It therefore
//! sits beside the row queries rather than inside them, holds [`AppState`]
//! rather than a `ReadStore`, and is not subscribable — a socket that offered
//! to push raft membership would be offering to push something the change feed
//! has never heard of.
//!
//! It is on the same `/api/q/` prefix all the same, because from a bundle's
//! side it is the same thing: a cacheless GET that reads and never writes.
//! axum prefers the literal segment over `/api/q/{name}`, so the two coexist
//! without either knowing about the other.
//!
//! The guard is the operator relation, re-read here exactly as the platform
//! row queries re-read it ([`query::platform::guard`]): a console that told a
//! member which node was leader would be telling them about a deployment they
//! hold nothing in.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use rn_api::ClusterView;
use rn_kernel::feed::Feed;

use super::{decline, query};
use crate::auth::session::Session;
use crate::ops;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/q/cluster", get(cluster))
}

async fn cluster(State(state): State<AppState>, session: Session) -> Response {
    if let Err(error) = query::platform::guard(&state.store.reads(), &session.principal).await {
        return decline::from_kernel(&error);
    }
    Json(view(&state).await).into_response()
}

/// The node's own account of itself.
///
/// Every field is something this process actually witnessed. The feed head
/// comes off the feed rather than off a `SELECT` of this module's own, because
/// rung 6 swaps the transport and a reader with its own statement would go on
/// reading a table the new feed no longer owns; a feed that cannot answer
/// reads as zero, which is the same thing a subscriber seeding from scratch
/// would see.
pub async fn view(state: &AppState) -> ClusterView {
    ClusterView {
        node: state.node.to_string(),
        version: state.version.to_string(),
        raft: ops::raft_groups(state).await,
        feed_head: state.feed.head().await.unwrap_or_else(|error| {
            tracing::warn!(%error, "the change feed head could not be read");
            0
        }),
        subscribers: state.connections.live(),
        observations_pending: state.observations.len(),
    }
}
