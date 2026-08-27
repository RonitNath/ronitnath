//! `/admin/*` — the same actions on a **running** node, requirement G21.2.
//!
//! This router is never merged into [`crate::router`]. It is served by its own
//! listener, on its own socket, bound to the address
//! `RN_SITE__ADMIN_ADDR` names — and the configuration refuses any address
//! that is not loopback, at boot, in both modes
//! ([`crate::config::AppConfig::validate`]).
//!
//! **Reaching the socket is the authority, and that is defensible only because
//! it is loopback.** There is no cookie and no session here: nothing to steal,
//! nothing to expire, nothing to phish, because there is no credential at all.
//! What authorises a caller is that it could open a connection to 127.0.0.1,
//! which on a host means being on the host. One interface further out the same
//! sentence authorises the internet, which is why the refusal is at boot and
//! not at the request.
//!
//! `crates/server/tests/surface.rs` carries a row for every pattern below
//! asserting `404` from the public router for all six principals. That is the
//! stronger statement than "guarded": these are not routes the public surface
//! refuses, they are routes it does not have.
//!
//! The route set is the smallest that unwedges a deployment, and it is
//! deliberately smaller than the CLI's. `backup` and `restore` are absent
//! because a logical walk taken while commits are landing is consistent at no
//! offset at all; `wipe` is absent because emptying a *running* deployment
//! over an unauthenticated socket is not a recovery tool.

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use super::{Action, AdminError};
use crate::state::AppState;

/// What every action takes: who is accountable, and the action's own fields.
#[derive(Debug, Default, Deserialize)]
pub struct AdminParams {
    /// `--as`, spelled for a query string.
    #[serde(rename = "as")]
    pub who: Option<String>,
    /// An address or a public id, for the actions that name a person.
    pub person: Option<String>,
    /// A session's public id.
    pub session: Option<String>,
    /// A product slug.
    pub slug: Option<String>,
    /// Why, for the two delegations.
    pub reason: Option<String>,
    /// How many audit rows.
    pub tail: Option<usize>,
}

/// The node-local router. Mounted by `main` on its own listener, never by
/// [`crate::router`].
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/readyz", get(readyz))
        .route("/admin/audit", get(audit))
        .route("/admin/sessions", get(sessions))
        .route("/admin/operators", get(operators))
        .route("/admin/revoke-session", post(revoke_session))
        .route("/admin/grant-operator", post(grant_operator))
        .route("/admin/revoke-operator", post(revoke_operator))
        .route("/admin/products", get(products))
        .route("/admin/enable", post(enable))
        .route("/admin/disable", post(disable))
}

/// `/readyz` in full, plus the one number the public route does not carry: the
/// feed head.
///
/// It is here rather than on `/readyz` because [`crate::ops`] is another leg's
/// (B4), and because this is the reader that needs it: after a restore, the
/// head *is* the manifest's offset, and an operator confirming a restore has
/// nothing else to compare against.
async fn readyz(State(state): State<AppState>) -> Response {
    let raft = crate::ops::raft_groups(&state).await;
    let head = super::backup::head(&state).await.ok();
    Json(json!({
        "status": if raft.ready() { "ok" } else { "unavailable" },
        "node": state.node.to_string(),
        "version": state.version.to_string(),
        "feed_head": head,
        "raft": raft,
    }))
    .into_response()
}

async fn audit(State(state): State<AppState>, Query(params): Query<AdminParams>) -> Response {
    serve(
        &state,
        &params,
        Action::Audit {
            tail: params.tail.unwrap_or(20),
        },
    )
    .await
}

async fn sessions(State(state): State<AppState>, Query(params): Query<AdminParams>) -> Response {
    let action = Action::Sessions {
        person: params.person.clone(),
    };
    serve(&state, &params, action).await
}

async fn operators(State(state): State<AppState>, Query(params): Query<AdminParams>) -> Response {
    serve(&state, &params, Action::Operators).await
}

async fn products(State(state): State<AppState>, Query(params): Query<AdminParams>) -> Response {
    serve(&state, &params, Action::Products).await
}

async fn revoke_session(
    State(state): State<AppState>,
    Query(params): Query<AdminParams>,
) -> Response {
    match required(&params.session, "session") {
        Ok(session) => serve(&state, &params, Action::RevokeSession { session }).await,
        Err(missing) => wanted(missing),
    }
}

async fn grant_operator(
    State(state): State<AppState>,
    Query(params): Query<AdminParams>,
) -> Response {
    delegate(state, params, true).await
}

async fn revoke_operator(
    State(state): State<AppState>,
    Query(params): Query<AdminParams>,
) -> Response {
    delegate(state, params, false).await
}

async fn delegate(state: AppState, params: AdminParams, grant: bool) -> Response {
    let (person, reason) = match (
        required(&params.person, "person"),
        required(&params.reason, "reason"),
    ) {
        (Ok(person), Ok(reason)) => (person, reason),
        (Err(missing), _) | (_, Err(missing)) => return wanted(missing),
    };
    let action = if grant {
        Action::GrantOperator { person, reason }
    } else {
        Action::RevokeOperator { person, reason }
    };
    serve(&state, &params, action).await
}

async fn enable(State(state): State<AppState>, Query(params): Query<AdminParams>) -> Response {
    product(state, params, true).await
}

async fn disable(State(state): State<AppState>, Query(params): Query<AdminParams>) -> Response {
    product(state, params, false).await
}

async fn product(state: AppState, params: AdminParams, enabled: bool) -> Response {
    match required(&params.slug, "slug") {
        Ok(slug) => serve(&state, &params, Action::SetProduct { slug, enabled }).await,
        Err(missing) => wanted(missing),
    }
}

/// Run an action and answer with it.
///
/// The failures are *named*, unlike everything on the public surface. The
/// uniform decline exists so that a stranger cannot enumerate the deployment,
/// and there is no stranger on this socket — a caller that reached it is on
/// the host, and telling them nothing would leave them guessing at a recovery.
async fn serve(state: &AppState, params: &AdminParams, action: Action) -> Response {
    match super::run(state, params.who.as_deref(), &action).await {
        Ok(report) => {
            let mut body = report.body;
            if let (Some(offset), Some(fields)) = (report.offset, body.as_object_mut()) {
                fields.insert("offset".to_owned(), offset.into());
            }
            Json(body).into_response()
        }
        Err(error) => {
            let status = match error {
                AdminError::Usage(_) => axum::http::StatusCode::BAD_REQUEST,
                AdminError::NoSuchPerson(_) => axum::http::StatusCode::NOT_FOUND,
                AdminError::NotYet(_) => axum::http::StatusCode::NOT_IMPLEMENTED,
                AdminError::Command(_) | AdminError::NoActor(_) | AdminError::Refused(_) => {
                    axum::http::StatusCode::FORBIDDEN
                }
                AdminError::Locked { .. } | AdminError::Store(_) => {
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                }
            };
            (status, Json(json!({ "error": error.to_string() }))).into_response()
        }
    }
}

/// A parameter this action cannot run without, or the name it is missing.
fn required<'a>(value: &Option<String>, name: &'a str) -> Result<String, &'a str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or(name)
}

/// The answer to a request that left one out.
fn wanted(name: &str) -> Response {
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(json!({ "error": format!("this action wants ?{name}=") })),
    )
        .into_response()
}

/// Serve `/admin/*` on the configured loopback address, until the process
/// ends.
///
/// Returns `None` when no address is configured, which is the default and the
/// state every deployment is in until somebody decides otherwise. A bind
/// failure is logged and *not* fatal: the admin listener is a recovery tool,
/// and a deployment that refused to serve its readers because its recovery
/// tool could not get a port would have the priorities exactly backwards.
#[must_use]
pub fn spawn(state: AppState) -> Option<tokio::task::JoinHandle<()>> {
    let addr = state.config.admin_addr?;
    debug_assert!(
        addr.ip().is_loopback(),
        "the configuration refuses a non-loopback admin address at load"
    );
    Some(tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::error!(%addr, %error, "the admin listener could not bind; /admin/* is not served");
                return;
            }
        };
        tracing::warn!(
            %addr,
            "serving /admin/* on loopback: reaching this socket is full operator authority, and \
             every action still writes an audit row naming its ?as="
        );
        let app = router().with_state(state);
        if let Err(error) = axum::serve(listener, app)
            .with_graceful_shutdown(crate::shutdown::signal())
            .await
        {
            tracing::error!(%error, "the admin listener stopped with an error");
        }
    }))
}
