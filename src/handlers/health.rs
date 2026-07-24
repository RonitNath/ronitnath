//! Liveness/version endpoint.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct Health {
    service: &'static str,
    status: &'static str,
    version: &'static str,
    git_hash: &'static str,
    uptime_secs: u64,
}

/// Liveness and version info — point monitors or agents at this.
#[utoipa::path(
    get,
    path = "/healthz",
    tag = "observability",
    responses((status = 200, description = "Service is up", body = Health))
)]
pub async fn healthz(State(state): State<AppState>) -> Json<Health> {
    Json(Health {
        service: "ronitnath",
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        git_hash: crate::view::release_revision(),
        uptime_secs: state.uptime().as_secs(),
    })
}
