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
//!
//! A `422` is the opposite case and is kept whole. The server answers a
//! validation failure with `{"invalid":[{"field":…,"message":…}]}` precisely so
//! a form can put the complaint next to the control that produced it — owner
//! ruling, "frontend messages should be in the appropriate location" — so this
//! client decodes that body into [`ApiError::Invalid`] rather than dropping it
//! on the floor as a status number.

use gloo_net::http::Request;
use serde::Serialize;
use serde::de::DeserializeOwned;

use web_sys::{RequestCredentials, RequestMode};

use rn_api::{Command, CommandEnvelope, CommandReply, Whoami};

/// One field the server would not accept, and what it said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    /// The field the server named. A form matches on it.
    pub field: String,
    /// What was wrong with it, in the reader's terms.
    pub message: String,
}

/// Why a call did not produce a value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    /// The server declined: no such thing, or not yours. Both, deliberately.
    #[error("declined")]
    Declined,
    /// The request was understood and refused for its shape, field by field.
    #[error("{}", one_line(.0))]
    Invalid(Vec<Invalid>),
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

impl ApiError {
    /// What the server said about one field, if it named one.
    ///
    /// This is what a field's note renders: a complaint about `email` belongs
    /// beside the email input and nowhere else.
    ///
    /// The comparison is on the *name*, not on the spelling of it. The kernel
    /// writes `display name` where a form calls its control `display_name`,
    /// and a note that rendered nowhere because of a space would be exactly
    /// the failure the field is carried to prevent.
    #[must_use]
    pub fn about(&self, field: &str) -> Option<String> {
        match self {
            Self::Invalid(fields) => fields
                .iter()
                .find(|invalid| same_field(&invalid.field, field))
                .map(|invalid| invalid.message.clone()),
            _ => None,
        }
    }

    /// What to show when the refusal has no field of its own to sit beside.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Declined => "Declined.".to_owned(),
            Self::Conflict => "That was already sent, differently.".to_owned(),
            Self::Unreachable(_) => "The server did not answer.".to_owned(),
            other => other.to_string(),
        }
    }
}

/// Whether two names name the same control. A space, an underscore and a
/// hyphen are the same separator, and case is not a distinction anybody meant.
fn same_field(server: &str, form: &str) -> bool {
    let plain = |name: &str| {
        name.chars()
            .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    plain(server) == plain(form)
}

/// Every complaint, as one sentence — the fallback when there is no field to
/// hang them on.
fn one_line(fields: &[Invalid]) -> String {
    if fields.is_empty() {
        return "the request was refused".to_owned();
    }
    fields
        .iter()
        .map(|invalid| invalid.message.clone())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Read the server's per-field complaints out of a `422` body.
///
/// A body that is not the shape the contract promises is a decline rather than
/// an empty complaint list: a form with nothing to say beside any field would
/// otherwise silently show nothing at all.
fn invalid(body: &str) -> ApiError {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return ApiError::Declined;
    };
    let Some(list) = value.get("invalid").and_then(serde_json::Value::as_array) else {
        return ApiError::Declined;
    };
    let fields: Vec<Invalid> = list
        .iter()
        .filter_map(|item| {
            Some(Invalid {
                field: item.get("field")?.as_str()?.to_owned(),
                message: item.get("message")?.as_str()?.to_owned(),
            })
        })
        .collect();
    if fields.is_empty() {
        return ApiError::Declined;
    }
    ApiError::Invalid(fields)
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

/// The highest offset this bundle has been told about.
///
/// Sent back as the envelope's `after` so the node that runs the next command
/// waits until it has applied at least that far before reading anything. A
/// bundle that talks to one node through an edge is not guaranteed to keep
/// talking to the same one, and a command whose preconditions are read one
/// commit behind is declined for a world the caller has already seen past
/// (finding 2, `docs/perf/2026-08-27.md`).
///
/// A `Cell` rather than an atomic: a bundle is one thread, and the value is a
/// hint whose only failure mode is waiting for nothing.
fn seen() -> &'static std::thread::LocalKey<std::cell::Cell<u64>> {
    thread_local! {
        static SEEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    &SEEN
}

/// Tell the client an offset it has been shown — a subscription's diff, say,
/// which is the other way a bundle learns where the feed is.
pub fn observed(offset: u64) {
    seen().with(|cell| cell.set(cell.get().max(offset)));
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
    let body = CommandEnvelope::after(args, seen().with(std::cell::Cell::get));
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
    observed(reply.offset);
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
        // The only status whose body a caller is allowed to read: it names
        // fields the caller sent, and nothing about what else exists.
        422 => Err(match response.text().await {
            Ok(body) => invalid(&body),
            Err(_) => ApiError::Declined,
        }),
        other => Err(ApiError::Status(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_422_body_becomes_a_complaint_per_field() {
        let error = invalid(
            r#"{"invalid":[{"field":"email","message":"That is not an address."},
                           {"field":"password","message":"Too short."}]}"#,
        );
        assert_eq!(
            error.about("email").as_deref(),
            Some("That is not an address.")
        );
        assert_eq!(error.about("password").as_deref(), Some("Too short."));
        assert_eq!(error.about("display_name"), None, "a field nobody named");
        assert_eq!(error.message(), "That is not an address. Too short.");
    }

    #[test]
    fn a_field_is_matched_by_name_and_not_by_spelling() {
        // The kernel writes the human phrase; a form names its control.
        let error =
            invalid(r#"{"invalid":[{"field":"display name","message":"A name is required."}]}"#);
        assert_eq!(
            error.about("display_name").as_deref(),
            Some("A name is required."),
            "a space is not a different field from an underscore"
        );
        assert_eq!(
            error.about("Display Name").as_deref(),
            Some("A name is required.")
        );
        assert_eq!(error.about("name"), None, "and it is still the same name");
    }

    #[test]
    fn a_body_the_contract_does_not_promise_is_a_plain_decline() {
        for body in [
            "not json at all",
            r#"{"error":"nope"}"#,
            r#"{"invalid":[]}"#,
            r#"{"invalid":[{"field":"email"}]}"#,
        ] {
            assert_eq!(invalid(body), ApiError::Declined, "{body}");
        }
    }

    #[test]
    fn a_refusal_with_no_field_still_has_something_to_say() {
        assert_eq!(ApiError::Declined.message(), "Declined.");
        assert_eq!(ApiError::Declined.about("email"), None);
        assert_eq!(
            ApiError::Unreachable("socket".into()).message(),
            "The server did not answer.",
            "the reader is not shown the transport's own words"
        );
        assert_eq!(ApiError::Status(500).message(), "unexpected status 500");
    }
}
