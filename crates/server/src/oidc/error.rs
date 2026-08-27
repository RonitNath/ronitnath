//! The two shapes an OAuth failure takes, and the rule about which.
//!
//! A **redirect** error (RFC 6749 §4.1.2.1) goes back to the client with
//! `error`, `error_description`, `state` and `iss` in the query. A **direct**
//! error (§5.2) is a JSON body at the endpoint that was called. Which of the
//! two applies is not a matter of taste: the authorization endpoint may only
//! redirect once it has *established* the client and the redirect URI, because
//! before that it does not know where a redirect would go — and a redirect to
//! an unverified URI is an open redirect with the deployment's name on it. So
//! a failure to establish either renders a page instead.

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// The error codes this deployment emits, by the specification's own words.
///
/// Written out rather than taken as a string, so a typo in a code is a
/// compile error and the discovery document's promises and the endpoint's
/// answers stay one vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// RFC 6749 §4.1.2.1 / §5.2 — the request is missing or malformed.
    InvalidRequest,
    /// §5.2 — client authentication failed. Answered `401`.
    InvalidClient,
    /// §5.2 — the code or refresh token is not good.
    InvalidGrant,
    /// §5.2 — the client may not use this grant.
    UnauthorizedClient,
    /// §5.2 — the grant type is not one this deployment implements.
    UnsupportedGrantType,
    /// §4.1.2.1 — `response_type` is not `code`.
    UnsupportedResponseType,
    /// §5.2 / §4.1.2.1 — a scope this deployment does not admit, or one this
    /// client may not ask for.
    InvalidScope,
    /// §4.1.2.1 — the person said no.
    AccessDenied,
    /// §4.1.2.1 — something broke on this side.
    ServerError,
    /// Core §3.1.2.6 — `prompt=none` and nobody is signed in.
    LoginRequired,
    /// Core §3.1.2.6 — `prompt=none` and consent has not been given.
    ConsentRequired,
    /// Core §3.1.2.6 — `prompt=none` and the request needs a person.
    InteractionRequired,
    /// Core §3.1.2.6 — `prompt=none` and the session is not the one asked for.
    AccountSelectionRequired,
    /// Core §6 — `request` is not supported.
    RequestNotSupported,
    /// Core §6 — `request_uri` is not supported.
    RequestUriNotSupported,
}

impl Code {
    /// The code, as the parameter or the JSON member carries it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidClient => "invalid_client",
            Self::InvalidGrant => "invalid_grant",
            Self::UnauthorizedClient => "unauthorized_client",
            Self::UnsupportedGrantType => "unsupported_grant_type",
            Self::UnsupportedResponseType => "unsupported_response_type",
            Self::InvalidScope => "invalid_scope",
            Self::AccessDenied => "access_denied",
            Self::ServerError => "server_error",
            Self::LoginRequired => "login_required",
            Self::ConsentRequired => "consent_required",
            Self::InteractionRequired => "interaction_required",
            Self::AccountSelectionRequired => "account_selection_required",
            Self::RequestNotSupported => "request_not_supported",
            Self::RequestUriNotSupported => "request_uri_not_supported",
        }
    }

    /// The status a direct error carries. `invalid_client` is the only `401`,
    /// and it is the one that also needs `WWW-Authenticate` (RFC 6749 §5.2).
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::InvalidClient => StatusCode::UNAUTHORIZED,
            Self::ServerError => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        }
    }
}

/// One failure: a code, and a sentence about the caller's own request.
#[derive(Debug, Clone)]
pub struct OauthError {
    /// Which code.
    pub code: Code,
    /// What is wrong with it.
    ///
    /// Safe to be specific for the same reason `Invalid` is
    /// (`api::decline`): it describes the shape of a request the caller
    /// itself sent. It never says whether a *row* exists — "no such code" and
    /// "not your code" are both `invalid_grant`, with the same sentence.
    pub description: &'static str,
}

impl OauthError {
    /// Build one.
    #[must_use]
    pub const fn new(code: Code, description: &'static str) -> Self {
        Self { code, description }
    }

    /// The direct form: a JSON body at the endpoint that was called.
    #[must_use]
    pub fn direct(&self) -> Response {
        let mut response = (
            self.code.status(),
            Json(json!({
                "error": self.code.as_str(),
                "error_description": self.description,
            })),
        )
            .into_response();
        no_store(&mut response);
        if self.code == Code::InvalidClient {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"oidc\", charset=\"UTF-8\""),
            );
        }
        response
    }
}

/// `Cache-Control: no-store` and `Pragma: no-cache`, which RFC 6749 §5.1
/// requires of every token-endpoint response — including the failures.
///
/// The `Cache-Control` header is set by the freshness layer as well, to the
/// same value for this path; `Pragma` is HTTP/1.0's and is nobody else's job.
pub fn no_store(response: &mut Response) {
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_spells_itself_out_and_only_one_of_them_is_a_401() {
        assert_eq!(Code::InvalidGrant.as_str(), "invalid_grant");
        assert_eq!(Code::InvalidClient.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(Code::InvalidGrant.status(), StatusCode::BAD_REQUEST);
        assert_eq!(Code::AccessDenied.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            Code::ServerError.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn a_bad_client_credential_is_answered_with_a_challenge() {
        let response = OauthError::new(Code::InvalidClient, "no").direct();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().contains_key(header::WWW_AUTHENTICATE));
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "no-store",
            "a token response is never stored"
        );
        assert_eq!(response.headers()[header::PRAGMA], "no-cache");
    }

    #[test]
    fn an_ordinary_failure_carries_no_challenge() {
        let response = OauthError::new(Code::InvalidGrant, "no").direct();
        assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
    }
}
