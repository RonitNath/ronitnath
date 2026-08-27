//! `POST /oidc/revoke` — RFC 7009.
//!
//! The specification's own instruction is the interesting part: an unknown
//! token is a **success**. A `200` for a token this deployment never minted is
//! what stops the endpoint being an oracle for which tokens exist, and it is
//! also the right answer — the caller asked for the token to stop working, and
//! it does not work.
//!
//! A refresh token takes its whole rotation family with it. Revoking one link
//! of a chain and leaving the rest is a revocation that lasts until the next
//! refresh.

use axum::Form;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use rn_api::commands::RevokeToken;
use rn_kernel::Principal;
use serde::Deserialize;

use super::error::{Code, OauthError, no_store};
use super::token::caller;
use crate::state::AppState;

/// The form (RFC 7009 §2.1).
#[derive(Debug, Default, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub token: String,
    /// A hint, and treated as one: this deployment looks the token up by its
    /// digest, which finds it whatever the hint said.
    #[serde(default)]
    pub token_type_hint: Option<String>,
    #[serde(flatten)]
    pub client: super::token::Request,
}

/// The route.
pub async fn post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<Request>,
) -> Response {
    let caller = match caller(&headers, &form.client) {
        Ok(caller) => caller,
        Err(error) => return error.direct(),
    };
    if form.token.trim().is_empty() {
        return OauthError::new(Code::InvalidRequest, "token is required").direct();
    }
    let args = RevokeToken {
        client_id: caller.id.clone(),
        client_secret: caller.secret.clone(),
        assertion: caller.assertion.clone(),
        token: form.token.clone(),
    };
    match rn_kernel::cmd::revoke_token(&super::context(&state, Principal::Anonymous), &args).await {
        Ok(_) => {
            let mut response = StatusCode::OK.into_response();
            no_store(&mut response);
            response
        }
        Err(error) => {
            // The only thing that reaches here is a client that could not
            // prove it is itself: an unknown token is a success by §2.2, and
            // the command writes its audit row either way.
            tracing::debug!(%error, "a revocation was refused");
            OauthError::new(
                Code::InvalidClient,
                "that client could not be authenticated",
            )
            .direct()
        }
    }
}
