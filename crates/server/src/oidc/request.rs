//! The authorization request: what arrived, and what it has to be before
//! anything else happens.
//!
//! The order of the checks here is the security property, not a style. The
//! client and the redirect URI are established *first* and by themselves,
//! because every later failure is answered by redirecting to that URI — and a
//! redirect to a URI this deployment has not verified is an open redirect with
//! this deployment's name on it (RFC 6749 §4.1.2.1 says so in as many words).
//! Until both hold, a failure renders a page.

use rn_api::oidc::Scope;
use serde::Deserialize;

use super::error::{Code, OauthError};

/// What `prompt` may say (OpenID Connect Core §3.1.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prompt {
    /// Nothing was asked for: interact if interaction is needed.
    #[default]
    Unset,
    /// Never interact. Every case that would is one of the four errors.
    None,
    /// Re-authenticate, however recently they signed in.
    Login,
    /// Ask again, however completely they have already agreed.
    Consent,
    /// Let them pick an account.
    SelectAccount,
}

impl Prompt {
    /// Read the parameter, refusing a word the specification does not define.
    ///
    /// `none` with anything else is invalid by §3.1.2.1, and is refused here
    /// rather than resolved into whichever the code happened to check first.
    pub fn parse(raw: Option<&str>) -> Result<Self, OauthError> {
        let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
            return Ok(Self::Unset);
        };
        let words: Vec<&str> = raw.split_whitespace().collect();
        if words.len() > 1 && words.contains(&"none") {
            return Err(OauthError::new(
                Code::InvalidRequest,
                "prompt=none cannot be combined with another value",
            ));
        }
        match words.as_slice() {
            ["none"] => Ok(Self::None),
            ["login"] => Ok(Self::Login),
            ["consent"] => Ok(Self::Consent),
            ["select_account"] => Ok(Self::SelectAccount),
            _ => Err(OauthError::new(
                Code::InvalidRequest,
                "prompt is one of none, login, consent, select_account",
            )),
        }
    }
}

/// The query the authorization endpoint receives, undigested.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Query {
    #[serde(default)]
    pub response_type: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub redirect_uri: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub code_challenge: Option<String>,
    #[serde(default)]
    pub code_challenge_method: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub max_age: Option<i64>,
    #[serde(default)]
    pub login_hint: Option<String>,
    #[serde(default)]
    pub id_token_hint: Option<String>,
    /// Declared unsupported, and refused rather than ignored.
    #[serde(default)]
    pub request: Option<String>,
    /// The same.
    #[serde(default)]
    pub request_uri: Option<String>,
}

/// A request that has passed everything but the person.
#[derive(Debug, Clone)]
pub struct Authorization {
    /// The client, by its public id — which is its `client_id`.
    pub client_id: String,
    /// Where the answer lands, already matched against the registration.
    pub redirect_uri: String,
    /// What is being asked for. Contains `openid`.
    pub scopes: Vec<Scope>,
    /// The RP's CSRF value, returned unchanged on every answer.
    pub state: Option<String>,
    /// The RP's replay nonce, carried into the id token.
    pub nonce: Option<String>,
    /// The S256 challenge.
    pub code_challenge: String,
    /// What interaction the RP asked for.
    pub prompt: Prompt,
    /// The oldest authentication the RP will accept.
    pub max_age: Option<i64>,
    /// Whose address to prefill a sign-in form with.
    pub login_hint: Option<String>,
    /// The id token naming the session the RP believes is current.
    pub id_token_hint: Option<String>,
}

impl Query {
    /// Everything about the request that is not about the person.
    ///
    /// Called *after* the client and the redirect URI have been established,
    /// so every failure here may be answered by redirecting.
    pub fn digest(&self, allowed: &[Scope]) -> Result<Authorization, OauthError> {
        if self.request.is_some() {
            return Err(OauthError::new(
                Code::RequestNotSupported,
                "this deployment does not accept request objects",
            ));
        }
        if self.request_uri.is_some() {
            return Err(OauthError::new(
                Code::RequestUriNotSupported,
                "this deployment does not accept request URIs",
            ));
        }
        if self.response_type.trim() != "code" {
            return Err(OauthError::new(
                Code::UnsupportedResponseType,
                "response_type is code; this deployment issues no other",
            ));
        }
        let Some(scopes) = Scope::parse_list(&self.scope) else {
            return Err(OauthError::new(
                Code::InvalidScope,
                "that scope list contains a scope this deployment does not admit",
            ));
        };
        if !scopes.contains(&Scope::Openid) {
            return Err(OauthError::new(
                Code::InvalidScope,
                "scope must contain openid",
            ));
        }
        if let Some(refused) = scopes.iter().find(|scope| !allowed.contains(scope)) {
            let _ = refused;
            return Err(OauthError::new(
                Code::InvalidScope,
                "that client is not registered for one of those scopes",
            ));
        }
        // PKCE, of every client type. RFC 9700 §2.1.1 asks for it of public
        // clients and recommends it of confidential ones; a deployment that
        // required it of only half would have a downgrade for the other half.
        let Some(challenge) = self.code_challenge.as_deref().filter(|c| !c.is_empty()) else {
            return Err(OauthError::new(
                Code::InvalidRequest,
                "code_challenge is required, with code_challenge_method=S256",
            ));
        };
        // `plain` is the other method RFC 7636 defines and it is not one this
        // deployment accepts: it protects against nothing an interceptor of
        // the authorization request cannot already do.
        if self.code_challenge_method.as_deref() != Some("S256") {
            return Err(OauthError::new(
                Code::InvalidRequest,
                "code_challenge_method must be S256",
            ));
        }
        if self.max_age.is_some_and(|age| age < 0) {
            return Err(OauthError::new(
                Code::InvalidRequest,
                "max_age is a number of seconds",
            ));
        }
        Ok(Authorization {
            client_id: self.client_id.clone(),
            redirect_uri: self.redirect_uri.clone(),
            scopes,
            state: self.state.clone(),
            nonce: self.nonce.clone(),
            code_challenge: challenge.to_owned(),
            prompt: Prompt::parse(self.prompt.as_deref())?,
            max_age: self.max_age,
            login_hint: self.login_hint.clone(),
            id_token_hint: self.id_token_hint.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> Query {
        Query {
            response_type: "code".into(),
            client_id: "c_x".into(),
            redirect_uri: "https://app.test/cb".into(),
            scope: "openid email".into(),
            code_challenge: Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".into()),
            code_challenge_method: Some("S256".into()),
            ..Query::default()
        }
    }

    const ALLOWED: &[Scope] = &[Scope::Openid, Scope::Email, Scope::Profile];

    #[test]
    fn a_well_formed_request_digests() {
        let digested = query().digest(ALLOWED).expect("it digests");
        assert_eq!(digested.scopes, vec![Scope::Openid, Scope::Email]);
        assert_eq!(digested.prompt, Prompt::Unset);
    }

    #[test]
    fn every_refusal_names_the_code_the_specification_gives_it() {
        /// One way to break a request, and the code it earns.
        type Case = (fn(&mut Query), Code);
        let cases: &[Case] = &[
            (
                |q| q.response_type = "token".into(),
                Code::UnsupportedResponseType,
            ),
            (|q| q.scope = "email".into(), Code::InvalidScope),
            (|q| q.scope = "openid nonsense".into(), Code::InvalidScope),
            (|q| q.scope = "openid groups".into(), Code::InvalidScope),
            (|q| q.code_challenge = None, Code::InvalidRequest),
            (
                |q| q.code_challenge_method = Some("plain".into()),
                Code::InvalidRequest,
            ),
            (|q| q.code_challenge_method = None, Code::InvalidRequest),
            (|q| q.max_age = Some(-1), Code::InvalidRequest),
            (|q| q.request = Some("ey".into()), Code::RequestNotSupported),
            (
                |q| q.request_uri = Some("https://a".into()),
                Code::RequestUriNotSupported,
            ),
            (|q| q.prompt = Some("nonsense".into()), Code::InvalidRequest),
            (
                |q| q.prompt = Some("none login".into()),
                Code::InvalidRequest,
            ),
        ];
        for (break_it, expected) in cases {
            let mut q = query();
            break_it(&mut q);
            let error = q.digest(ALLOWED).expect_err("it is refused");
            assert_eq!(error.code, *expected, "{q:?}");
        }
    }

    #[test]
    fn a_request_object_is_refused_before_anything_else_is_even_read() {
        let mut q = Query {
            request: Some("ey".into()),
            ..Query::default()
        };
        // Nothing else about this request is valid either; the answer is
        // still the one the specification names for the parameter.
        assert_eq!(
            q.digest(ALLOWED).expect_err("refused").code,
            Code::RequestNotSupported
        );
        q.request = None;
        q.request_uri = Some("https://a".into());
        assert_eq!(
            q.digest(ALLOWED).expect_err("refused").code,
            Code::RequestUriNotSupported
        );
    }

    #[test]
    fn prompt_reads_only_the_four_words_it_has() {
        for (raw, expected) in [
            (None, Prompt::Unset),
            (Some(""), Prompt::Unset),
            (Some("none"), Prompt::None),
            (Some("login"), Prompt::Login),
            (Some("consent"), Prompt::Consent),
            (Some("select_account"), Prompt::SelectAccount),
        ] {
            assert_eq!(Prompt::parse(raw).ok(), Some(expected), "{raw:?}");
        }
        assert!(Prompt::parse(Some("login consent")).is_err());
        assert!(Prompt::parse(Some("nonsense")).is_err());
    }
}
