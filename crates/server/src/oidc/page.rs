//! The two pages the Provider renders, and the one thing they have in common:
//! they are doors.
//!
//! A consent page that explains itself is a consent page nobody reads. This
//! one names the client, lists the scopes as plain words, and offers two
//! buttons. The deployment's own name comes from configuration
//! (`RN_SITE__PUBLIC_NAME`, or the issuer's host) rather than from a constant,
//! because the Provider is a feature of the platform and not of one site.

use askama::Template;
use askama_web::WebTemplate;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use rn_api::oidc::Scope;

use crate::presence::theme_for;
use crate::state::AppState;

/// What one scope is, in a sentence a reader can decide about.
///
/// Plain words and no jargon: the reader is being asked to agree to
/// something, and "openid" is not something anybody can agree to.
#[must_use]
pub const fn describe(scope: Scope) -> &'static str {
    match scope {
        Scope::Openid => "Know who you are",
        Scope::Profile => "Your name and handle",
        Scope::Email => "Your email address",
        Scope::Groups => "Which of this organization's groups you are in",
        Scope::OfflineAccess => "Stay signed in when you are away",
    }
}

/// The consent page.
#[derive(Template, WebTemplate)]
#[template(path = "consent.html")]
pub struct ConsentPage {
    pub theme: &'static str,
    pub version: String,
    /// What this deployment calls itself.
    pub deployment: String,
    /// The client asking, by its registered name.
    pub client: String,
    /// Its home page, if it registered one.
    pub client_uri: String,
    /// What it is asking for, as words.
    pub scopes: Vec<&'static str>,
    /// The query the decision has to carry back, as hidden fields.
    pub fields: Vec<Field>,
}

/// The RP-initiated sign-out confirmation.
///
/// Shown when the request carries no `id_token_hint`, or one naming a session
/// that is not this browser's. Both are cases where an unconfirmed sign-out
/// would let any site end anybody's session by linking to this URL.
#[derive(Template, WebTemplate)]
#[template(path = "end_session.html")]
pub struct EndSessionPage {
    pub theme: &'static str,
    pub version: String,
    pub deployment: String,
    /// The client asking, if the request named one this deployment knows.
    pub client: String,
    /// The query the decision has to carry back.
    pub fields: Vec<Field>,
}

/// A failure the authorization endpoint may not redirect.
///
/// Two of them: a `client_id` this deployment does not know, and a
/// `redirect_uri` the registration does not name. Redirecting either would be
/// an open redirect, so they are rendered here instead — which is also the
/// only place a person sees an OAuth error in words rather than a parameter.
#[derive(Template, WebTemplate)]
#[template(path = "oidc_error.html")]
pub struct ErrorPage {
    pub theme: &'static str,
    pub version: String,
    pub deployment: String,
    /// The code, so a developer reading over somebody's shoulder can act.
    pub code: &'static str,
    /// What is wrong, in the caller's terms.
    pub message: &'static str,
}

/// One hidden field: the request, carried across the decision.
pub struct Field {
    /// The parameter's name.
    pub name: &'static str,
    /// Its value.
    pub value: String,
}

impl Field {
    /// A field, or nothing when the parameter was not sent — an empty hidden
    /// input is not the same as an absent parameter, and round-tripping one
    /// as the other would turn `prompt` absent into `prompt=`.
    #[must_use]
    pub fn some(name: &'static str, value: Option<&str>) -> Option<Self> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Self {
                name,
                value: value.to_owned(),
            })
    }

    /// A field whose value is always present.
    #[must_use]
    pub fn new(name: &'static str, value: impl Into<String>) -> Self {
        Self {
            name,
            value: value.into(),
        }
    }
}

/// The chrome every page here shares.
#[must_use]
pub fn chrome(state: &AppState, headers: &HeaderMap) -> (&'static str, String, String) {
    (
        theme_for(headers).as_str(),
        state.version.to_string(),
        state.config.display_name(),
    )
}

/// The error page as a response. `400`, because the request was.
#[must_use]
pub fn refuse(state: &AppState, headers: &HeaderMap, error: &super::error::OauthError) -> Response {
    let (theme, version, deployment) = chrome(state, headers);
    (
        StatusCode::BAD_REQUEST,
        ErrorPage {
            theme,
            version,
            deployment,
            code: error.code.as_str(),
            message: error.description,
        },
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scope_has_words_a_reader_can_decide_about() {
        for scope in Scope::ALL {
            let words = describe(scope);
            assert!(!words.is_empty());
            assert!(
                !words.contains('_') && words != scope.as_str(),
                "{scope:?} is described by its own parameter name"
            );
        }
    }

    #[test]
    fn an_absent_parameter_does_not_become_an_empty_one() {
        assert!(Field::some("state", None).is_none());
        assert!(Field::some("state", Some("  ")).is_none());
        assert_eq!(Field::some("state", Some("x")).expect("a field").value, "x");
    }
}
