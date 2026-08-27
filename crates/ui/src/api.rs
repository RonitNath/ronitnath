//! The API client: the three calls a bundle is allowed to make.
//!
//! `docs/rebuild/plan.md` §API is the contract — queries read, commands write
//! and carry an idempotency key, and `whoami` is the only chrome DTO. Every
//! request is same-origin by construction: the URLs are relative and the
//! fetch is pinned to `same-origin` mode and credentials, so a bundle served
//! from somewhere it should not be fails to talk rather than leaking a cookie.
//!
//! A `403` and a `404` are the same event here — [`ApiError::Declined`] — for
//! the same reason the server sends the same bytes for both: telling them
//! apart is how a caller enumerates what exists.

use gloo_net::http::Request;
use serde::Serialize;
use serde::de::DeserializeOwned;

use web_sys::{RequestCredentials, RequestMode};

use rn_api::{Command, CommandEnvelope, CommandReply, Whoami};

/// Why a call did not produce a value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    /// The server declined: no such thing, or not yours. Both, deliberately.
    #[error("declined")]
    Declined,
    /// The idempotency key was replayed with a different body. The caller
    /// changed its mind mid-retry; mint a new key.
    #[error("the idempotency key was replayed with a different body")]
    Conflict,
    /// The request never got an answer.
    #[error("the server is unreachable: {0}")]
    Unreachable(String),
    /// The answer did not have the shape the wire types promise.
    #[error("the reply did not parse: {0}")]
    Malformed(String),
    /// Any other status. Carried as-is so a caller can log it.
    #[error("unexpected status {0}")]
    Status(u16),
}

/// A command's reply: the change-feed offset it landed at, and whatever it
/// returned. Waiting for a subscription to reach `offset` is how a caller sees
/// its own write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed<T> {
    /// The offset the command's event was appended at.
    pub offset: u64,
    /// What the command returned, decoded.
    pub result: T,
}

/// `GET /api/q/<name>?…`
///
/// Queries never write — the server enforces that with a store type that has
/// no `execute` — so a failed query is always safe to retry.
pub async fn query<T: DeserializeOwned>(
    name: &str,
    params: &[(&str, &str)],
) -> Result<T, ApiError> {
    get(&format!("/api/q/{name}"), params).await
}

/// `POST /api/cmd/<name>` with a freshly minted idempotency key.
///
/// The key is minted here rather than by the caller so that every command in
/// the bundle is retry-safe by default: a dropped response can be re-sent
/// verbatim and the server replays its original reply.
pub async fn command<A: Serialize, T: DeserializeOwned>(
    name: &str,
    args: A,
) -> Result<Committed<T>, ApiError> {
    let body = CommandEnvelope::new(args);
    let request = same_origin(Request::post(&format!("/api/cmd/{name}")))
        .json(&body)
        .map_err(|e| ApiError::Malformed(e.to_string()))?;
    let response = request
        .send()
        .await
        .map_err(|e| ApiError::Unreachable(e.to_string()))?;
    let reply: CommandReply = decode(response).await?;
    let result = serde_json::from_value(reply.result)
        .map_err(|e| ApiError::Malformed(format!("command result: {e}")))?;
    Ok(Committed {
        offset: reply.offset,
        result,
    })
}

/// [`command`], with the route name taken from the args type instead of a
/// string — so a body cannot be posted to the wrong endpoint.
pub async fn invoke<C: Command + Serialize, T: DeserializeOwned>(
    args: C,
) -> Result<Committed<T>, ApiError> {
    command(C::NAME, args).await
}

/// `GET /api/whoami` — everything the chrome is allowed to know.
pub async fn whoami() -> Result<Whoami, ApiError> {
    get("/api/whoami", &[]).await
}

/// One GET, same-origin, decoded.
async fn get<T: DeserializeOwned>(url: &str, params: &[(&str, &str)]) -> Result<T, ApiError> {
    let response = same_origin(Request::get(url))
        .query(params.iter().copied())
        .send()
        .await
        .map_err(|e| ApiError::Unreachable(e.to_string()))?;
    decode(response).await
}

/// Pin a request to this origin, cookies included, nothing else.
fn same_origin(builder: gloo_net::http::RequestBuilder) -> gloo_net::http::RequestBuilder {
    builder
        .mode(RequestMode::SameOrigin)
        .credentials(RequestCredentials::SameOrigin)
}

/// Map a response onto the decline vocabulary, then parse it.
async fn decode<T: DeserializeOwned>(response: gloo_net::http::Response) -> Result<T, ApiError> {
    match response.status() {
        200..=299 => response
            .json()
            .await
            .map_err(|e| ApiError::Malformed(e.to_string())),
        403 | 404 => Err(ApiError::Declined),
        409 => Err(ApiError::Conflict),
        other => Err(ApiError::Status(other)),
    }
}
