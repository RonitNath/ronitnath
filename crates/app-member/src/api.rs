//! Posting a command from this bundle, and reading what came back when it was
//! refused.
//!
//! `rn_ui::api` is the client, and every rule it carries — relative URLs,
//! same-origin mode and credentials, an idempotency key per call — holds here
//! too. What it does not carry is the *body* of a `422`: the server answers a
//! validation failure with `{"invalid":[{"field":…,"message":…}]}` so a form
//! can render the complaint beside the field that produced it, and the shared
//! client folds that status into `ApiError::Status(422)` with the body
//! dropped. A member page is mostly forms, so this bundle reads the body
//! itself. It is the thinnest call site that keeps the field-level errors the
//! product contract promises, and the gap is rn-ui's to close.

use leptos::prelude::*;
use rn_api::{Command, CommandEnvelope, CommandReply};
use serde::Serialize;
use serde_json::Value;
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestCredentials, RequestInit, RequestMode, Response};

/// One field the server would not accept, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Complaint {
    /// The field the server named.
    pub field: String,
    /// What it said about it.
    pub message: String,
}

/// Why a command did not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No such thing, or not yours — the server does not say which, and
    /// neither does this.
    Declined,
    /// The request was understood and refused for its shape.
    Invalid(Vec<Complaint>),
    /// The idempotency key was replayed with a different body.
    Conflict,
    /// It never got an answer.
    Unreachable,
}

impl Refusal {
    /// What to show when the complaint has no field of its own to sit beside.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Declined => "Declined.".to_owned(),
            Self::Conflict => "That was already sent, differently.".to_owned(),
            Self::Unreachable => "The server did not answer.".to_owned(),
            Self::Invalid(complaints) => complaints
                .iter()
                .map(|complaint| complaint.message.clone())
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// What the server said about one field, if it said anything.
    #[must_use]
    pub fn about(&self, field: &str) -> Option<String> {
        match self {
            Self::Invalid(complaints) => complaints
                .iter()
                .find(|complaint| complaint.field == field)
                .map(|complaint| complaint.message.clone()),
            _ => None,
        }
    }
}

/// Post a command, and read the result it returned.
///
/// The route comes from the args type, so a body cannot reach the wrong
/// endpoint, and the key is minted per call, so a retry after a dropped
/// response replays rather than repeats.
pub async fn run<A: Command + Serialize>(args: &A) -> Result<Value, Refusal> {
    let body =
        serde_json::to_string(&CommandEnvelope::new(args)).map_err(|_| Refusal::Unreachable)?;
    let response = post(&format!("/api/cmd/{}", A::NAME), &body).await?;
    let text = text_of(&response).await.unwrap_or_default();
    match response.status() {
        200..=299 => serde_json::from_str::<CommandReply>(&text)
            .map(|reply| reply.result)
            .map_err(|_| Refusal::Unreachable),
        409 => Err(Refusal::Conflict),
        422 => Err(complaints(&text)),
        403 | 404 => Err(Refusal::Declined),
        _ => Err(Refusal::Unreachable),
    }
}

/// Run a command and put whatever it refused into a signal, so a page can show
/// the refusal beside the control that caused it.
pub fn attempt<A, F>(args: A, note: RwSignal<Option<Refusal>>, then: F)
where
    A: Command + Serialize + 'static,
    F: FnOnce(Value) + 'static,
{
    leptos::task::spawn_local(async move {
        match run(&args).await {
            Ok(result) => {
                note.set(None);
                then(result);
            }
            Err(refusal) => note.set(Some(refusal)),
        }
    });
}

/// Read the server's per-field complaints, or fall back to a bare refusal.
fn complaints(body: &str) -> Refusal {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Refusal::Declined;
    };
    let Some(list) = value.get("invalid").and_then(Value::as_array) else {
        return Refusal::Declined;
    };
    Refusal::Invalid(
        list.iter()
            .filter_map(|item| {
                Some(Complaint {
                    field: item.get("field")?.as_str()?.to_owned(),
                    message: item.get("message")?.as_str()?.to_owned(),
                })
            })
            .collect(),
    )
}

async fn post(url: &str, body: &str) -> Result<Response, Refusal> {
    let options = RequestInit::new();
    options.set_method("POST");
    options.set_mode(RequestMode::SameOrigin);
    options.set_credentials(RequestCredentials::SameOrigin);
    options.set_body(&JsValue::from_str(body));
    let request =
        Request::new_with_str_and_init(url, &options).map_err(|_| Refusal::Unreachable)?;
    request
        .headers()
        .set("content-type", "application/json")
        .map_err(|_| Refusal::Unreachable)?;
    let window = web_sys::window().ok_or(Refusal::Unreachable)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| Refusal::Unreachable)?;
    response
        .dyn_into::<Response>()
        .map_err(|_| Refusal::Unreachable)
}

async fn text_of(response: &Response) -> Option<String> {
    let text = JsFuture::from(response.text().ok()?).await.ok()?;
    text.as_string()
}
