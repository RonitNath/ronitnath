//! What this deployment says it is: the two metadata documents and the JWKS.
//!
//! Both metadata paths serve the same bytes. `/.well-known/openid-configuration`
//! is OpenID Connect Discovery 1.0's; `/.well-known/oauth-authorization-server`
//! is RFC 8414's, and a plain OAuth client looks there. One document rather
//! than two, because they describe one server and two that could disagree
//! would eventually.
//!
//! Every URL in it is built from the issuer, which is
//! `RN_SITE__PUBLIC_ORIGIN` exactly. Nothing here is a constant tied to one
//! deployment: a downstream binary that mounts this router and sets that
//! variable describes itself correctly without touching this file.
//!
//! The negatives are as load-bearing as the positives. `claims_parameter_supported`,
//! `request_parameter_supported` and `request_uri_parameter_supported` are all
//! `false`, and the endpoint refuses those parameters rather than ignoring
//! them — a server that silently drops a `request` object is a server that
//! answers a question nobody asked.

use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use rn_api::oidc::{ClientAuthMethod, GrantType, Scope};
use rn_kernel::oidc::{self, paths};
use serde_json::{Value, json};

use crate::api::decline;
use crate::state::AppState;

/// The claims `userinfo` and the id token may carry, for the document to
/// declare. One list, so a claim this deployment does not produce is not
/// advertised and a claim it does is.
const CLAIMS: &[&str] = &[
    "iss",
    "sub",
    "aud",
    "exp",
    "iat",
    "auth_time",
    "nonce",
    "sid",
    "azp",
    "name",
    "preferred_username",
    "updated_at",
    "email",
    "email_verified",
    "groups",
];

/// The metadata document.
#[must_use]
pub fn document(state: &AppState) -> Value {
    let provider = state.provider.as_ref();
    let words = |list: &[&'static str]| Value::from(list.to_vec());
    json!({
        "issuer": provider.issuer(),
        "authorization_endpoint": provider.endpoint(paths::AUTHORIZE),
        "token_endpoint": provider.endpoint(paths::TOKEN),
        "userinfo_endpoint": provider.endpoint(paths::USERINFO),
        "jwks_uri": provider.endpoint(paths::JWKS),
        "revocation_endpoint": provider.endpoint(paths::REVOKE),
        "end_session_endpoint": provider.endpoint(paths::END_SESSION),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": words(&GrantType::ALL.map(GrantType::as_str)),
        "subject_types_supported": ["pairwise"],
        "id_token_signing_alg_values_supported": [oidc::jwt::ALG],
        "scopes_supported": words(&Scope::ALL.map(Scope::as_str)),
        "claims_supported": words(CLAIMS),
        "token_endpoint_auth_methods_supported":
            words(&ClientAuthMethod::ALL.map(ClientAuthMethod::as_str)),
        "token_endpoint_auth_signing_alg_values_supported": [oidc::jwt::ALG],
        "code_challenge_methods_supported": ["S256"],
        "claims_parameter_supported": false,
        "request_parameter_supported": false,
        "request_uri_parameter_supported": false,
        "authorization_response_iss_parameter_supported": true,
        "backchannel_logout_supported": true,
        "backchannel_logout_session_supported": true,
    })
}

/// `GET /.well-known/openid-configuration` and its RFC 8414 twin.
pub async fn metadata(State(state): State<AppState>) -> Response {
    Json(document(&state)).into_response()
}

/// `GET /oidc/jwks` — the active key and the retiring ones.
///
/// The retiring ones are the whole reason rotation is a non-event: a relying
/// party holding a cached document and a token signed a minute ago finds the
/// key it needs.
pub async fn jwks(State(state): State<AppState>) -> Response {
    match oidc::key::published(&state.store.reads()).await {
        Ok(keys) => Json(oidc::jwks_document(&keys)).into_response(),
        Err(error) => {
            tracing::error!(%error, "the signing keys could not be read");
            decline::forbidden()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declared_endpoint_is_under_the_issuer_and_at_the_path_it_is_served_on() {
        // The document is a function of the issuer alone, so it is checked
        // here against a provider rather than against a booted node.
        let provider = rn_kernel::oidc::Provider::new(
            "https://example.test/",
            rn_kernel::oidc::SealKey::mint(),
        );
        assert_eq!(
            provider.issuer(),
            "https://example.test",
            "no trailing slash"
        );
        assert_eq!(
            provider.endpoint(paths::TOKEN),
            "https://example.test/oidc/token"
        );
        assert_eq!(provider.token_endpoint(), provider.endpoint(paths::TOKEN));
    }
}
