//! `GET|POST /oidc/end_session` — RP-initiated logout.
//!
//! The parameter that matters is `id_token_hint`, and what it is for is
//! consent. Without it any site could sign anybody out of this deployment by
//! linking to this URL, which is a denial of service dressed as a feature — so
//! a request with no hint, or one naming a session that is not this browser's,
//! is shown a page and a button rather than obeyed.
//!
//! `post_logout_redirect_uri` is matched exactly against the client's
//! registration, and only when a hint identified the client. The specification
//! says as much, and the reason is the same one the authorization endpoint has:
//! an unverified redirect target is an open redirect carrying this
//! deployment's name.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Form, http::StatusCode};
use rn_api::commands::EndSession;
use rn_kernel::Principal;
use rn_kernel::oidc::{ClientRow, client as registry, subject};
use serde::Deserialize;

use super::page::{self, EndSessionPage, Field};
use super::url;
use crate::api::origin::SameOrigin;
use crate::auth::session::{self, Visitor};
use crate::state::AppState;

/// The request (RP-Initiated Logout §2).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id_token_hint: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub post_logout_redirect_uri: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    /// Read and not acted on: this deployment renders one language.
    #[serde(default)]
    pub ui_locales: Option<String>,
    /// Set by the confirmation page's own button, and by nothing else.
    #[serde(default)]
    pub confirmed: Option<String>,
}

/// `GET /oidc/end_session` — the link an RP sends somebody to.
pub async fn get(
    State(state): State<AppState>,
    visitor: Visitor,
    headers: HeaderMap,
    Query(request): Query<Request>,
) -> Response {
    run(&state, &visitor, &headers, request, false).await
}

/// `POST /oidc/end_session` — the confirmation, and the form-post form of the
/// same request.
pub async fn post(
    State(state): State<AppState>,
    _: SameOrigin,
    visitor: Visitor,
    headers: HeaderMap,
    Form(request): Form<Request>,
) -> Response {
    let confirmed = request.confirmed.is_some();
    run(&state, &visitor, &headers, request, confirmed).await
}

async fn run(
    state: &AppState,
    visitor: &Visitor,
    headers: &HeaderMap,
    request: Request,
    confirmed: bool,
) -> Response {
    let reads = state.store.reads();

    // The hint identifies the client and the person. It may be expired: it is
    // a hint about a session, not a credential for one.
    let hinted_sub = match &request.id_token_hint {
        Some(hint) => super::verified_hint(state, hint).await,
        None => None,
    };
    let client = named_client(state, &request, hinted_sub.as_ref()).await;

    // Is the hint about the browser that is here?
    let this_session = match (&hinted_sub, &visitor.principal) {
        (Some(hint), Principal::Member { person, .. }) => {
            let named = subject::person_of(&reads, &hint.sub).await.ok().flatten();
            named.is_some() && named == *person
        }
        _ => false,
    };

    // Nobody is signed in: there is nothing to end, and saying so is not a
    // leak — the reader is looking at their own browser.
    if !matches!(visitor.principal, Principal::Member { .. }) {
        return leave(state, &request, client.as_ref());
    }
    if !this_session && !confirmed {
        return confirm(state, headers, &request, client.as_ref());
    }

    let ctx = super::context(state, visitor.principal.clone());
    match rn_kernel::cmd::end_session(&ctx, &EndSession {}).await {
        Ok(committed) => {
            session::forget(state, headers);
            // The back-channel POSTs go off the request path: the reader is
            // already leaving, and an RP that is slow to answer must not be
            // the reason their sign-out hangs.
            super::logout::spawn(state, &committed.event);
            let mut response = leave(state, &request, client.as_ref());
            if let Ok(cookie) = session::clear(state.config.mode).parse() {
                response
                    .headers_mut()
                    .insert(axum::http::header::SET_COOKIE, cookie);
            }
            response
        }
        Err(error) => {
            tracing::warn!(%error, "an RP-initiated sign-out was refused");
            (
                StatusCode::BAD_REQUEST,
                "that sign-out could not be completed",
            )
                .into_response()
        }
    }
}

/// The client a request names, if it named one this deployment knows.
///
/// Either parameter may name it: `client_id` directly, or the `aud` of the
/// hint. Both go through the registry, so a name this deployment does not
/// know is no client at all rather than a string echoed back.
async fn named_client(
    state: &AppState,
    request: &Request,
    hinted: Option<&Hint>,
) -> Option<ClientRow> {
    let named = request
        .client_id
        .as_deref()
        .or(hinted.map(|hint| hint.audience.as_str()))?;
    let id = registry::decode_client_id(state.ids(), named).ok()?;
    registry::load(&state.store.reads(), id)
        .await
        .ok()
        .flatten()
}

/// Where the reader lands: the registered post-logout URI, or the landing
/// page.
///
/// The URI is honoured only when the client was identified *and* registered
/// it. Anything else goes to `/`, which is the answer to "sign me out and send
/// me somewhere I did not verify".
fn leave(state: &AppState, request: &Request, client: Option<&ClientRow>) -> Response {
    let _ = state;
    let Some(wanted) = request.post_logout_redirect_uri.as_deref() else {
        return Redirect::to("/").into_response();
    };
    let accepted = client.is_some_and(|client| client.accepts_post_logout(wanted));
    if !accepted {
        return Redirect::to("/").into_response();
    }
    let pairs: Vec<(&str, String)> = request
        .state
        .as_deref()
        .map(|sent| vec![("state", sent.to_owned())])
        .unwrap_or_default();
    Redirect::to(&url::with_query(wanted, &pairs)).into_response()
}

/// Ask before ending somebody's session.
fn confirm(
    state: &AppState,
    headers: &HeaderMap,
    request: &Request,
    client: Option<&ClientRow>,
) -> Response {
    let (theme, version, deployment) = page::chrome(state, headers);
    let fields: Vec<Field> = [
        Field::some("id_token_hint", request.id_token_hint.as_deref()),
        Field::some("client_id", request.client_id.as_deref()),
        Field::some(
            "post_logout_redirect_uri",
            request.post_logout_redirect_uri.as_deref(),
        ),
        Field::some("state", request.state.as_deref()),
        Some(Field::new("confirmed", "1")),
    ]
    .into_iter()
    .flatten()
    .collect();
    EndSessionPage {
        theme,
        version,
        deployment,
        client: client
            .map(|client| client.metadata.client_name.clone())
            .unwrap_or_default(),
        fields,
    }
    .into_response()
}

/// What a validated `id_token_hint` was worth.
pub struct Hint {
    /// Its `sub`.
    pub sub: String,
    /// Its `aud` — the client it was issued to.
    pub audience: String,
}
