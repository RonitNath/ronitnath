//! `POST /oidc/token` — where a code or a refresh token becomes an access
//! token, and where a client's secret is the only thing that matters.
//!
//! Client authentication is the kernel's, not this handler's: the credential
//! travels in the command's arguments and is checked against the method the
//! *registration* names ([`rn_kernel::oidc::client::authenticate`]). What this
//! file does is read the credential out of the three places RFC 6749 and RFC
//! 7523 put it, apply a rate limit so a secret cannot be ground down, and
//! shape the answer.
//!
//! Every response — success and failure alike — carries `Cache-Control:
//! no-store` and `Pragma: no-cache` (RFC 6749 §5.1).

use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use axum::{Form, Json};
use base64::Engine as _;
use rn_api::commands::{ClientCredentials, ExchangeCode, RefreshToken};
use rn_api::oidc::Scope;
use rn_kernel::Principal;
use rn_kernel::cmd::{Granted, Issued};
use serde::Deserialize;
use serde_json::json;

use super::error::{Code, OauthError, no_store};
use crate::state::AppState;

/// The `client_assertion_type` a `private_key_jwt` carries (RFC 7523 §2.2).
const ASSERTION_TYPE: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// The form the token endpoint receives.
#[derive(Debug, Default, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub grant_type: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub client_assertion: Option<String>,
    #[serde(default)]
    pub client_assertion_type: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub code_verifier: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Who the request says it is, from wherever it said it.
///
/// `Debug` prints neither credential. It exists so a test can name the value
/// it is asserting about, and a credential that could be printed is one that
/// eventually is.
pub struct Caller {
    /// The client id.
    pub id: String,
    /// A shared secret, from the header or the body.
    pub secret: Option<String>,
    /// A `private_key_jwt` assertion.
    pub assertion: Option<String>,
}

impl std::fmt::Debug for Caller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Caller")
            .field("id", &self.id)
            .field("secret", &self.secret.as_ref().map(|_| "redacted"))
            .field("assertion", &self.assertion.as_ref().map(|_| "redacted"))
            .finish()
    }
}

/// Read the client's identity and credential.
///
/// Three places, in the order RFC 6749 §2.3.1 puts them: the `Authorization:
/// Basic` header first, then the body. Presenting both is refused rather than
/// resolved — a request that authenticates twice is a request whose author
/// does not know which one is being checked.
pub fn caller(headers: &HeaderMap, form: &Request) -> Result<Caller, OauthError> {
    let basic = basic_auth(headers);
    if basic.is_some() && form.client_secret.is_some() {
        return Err(OauthError::new(
            Code::InvalidRequest,
            "a client authenticates once, in the header or in the body",
        ));
    }
    if form.client_assertion.is_some()
        && form.client_assertion_type.as_deref() != Some(ASSERTION_TYPE)
    {
        return Err(OauthError::new(
            Code::InvalidRequest,
            "client_assertion_type must be the JWT bearer URN",
        ));
    }
    let (id, secret) = match basic {
        Some((id, secret)) => (id, Some(secret)),
        None => (
            form.client_id.clone().unwrap_or_default(),
            form.client_secret.clone(),
        ),
    };
    if id.trim().is_empty() {
        return Err(OauthError::new(
            Code::InvalidClient,
            "no client_id was presented",
        ));
    }
    Ok(Caller {
        id,
        secret,
        assertion: form.client_assertion.clone(),
    })
}

/// The `Authorization: Basic` pair, percent-decoded per RFC 6749 §2.3.1.
fn basic_auth(headers: &HeaderMap) -> Option<(String, String)> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = raw.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (id, secret) = text.split_once(':')?;
    Some((form_decode(id), form_decode(secret)))
}

/// `application/x-www-form-urlencoded` decoding, which is what §2.3.1 says
/// the two halves of a Basic credential are encoded with.
fn form_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The route.
pub async fn post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<Request>,
) -> Response {
    let caller = match caller(&headers, &form) {
        Ok(caller) => caller,
        Err(error) => return error.direct(),
    };
    // A client secret is 256 bits and cannot be guessed; a *deployment* that
    // let one be tried without limit is still one whose token endpoint is a
    // free oracle. The bucket counts failures per client id and nothing else,
    // so a working client is never slowed down by a broken one.
    if !state
        .oidc_limits
        .allow(&caller.id, state.store.clock().now())
    {
        let mut response = OauthError::new(
            Code::InvalidClient,
            "too many failed attempts for that client",
        )
        .direct();
        *response.status_mut() = axum::http::StatusCode::TOO_MANY_REQUESTS;
        return response;
    }

    // Client authentication is the *command's* — the credential travels in
    // its arguments and is checked against the method the registration names.
    // It is asked here as well, and only to choose a status: a caller that
    // could not prove it is itself has to be told `invalid_client` with a
    // challenge (RFC 6749 §5.2), and a command that answered every refusal
    // with one word could not say which this was. The command still refuses
    // on its own; this is a diagnosis, not a gate.
    if let Err(error) = authenticated(&state, &caller).await {
        state
            .oidc_limits
            .failed(&caller.id, state.store.clock().now());
        return error.direct();
    }

    let granted = match form.grant_type.as_str() {
        "authorization_code" => authorization_code(&state, &caller, &form).await,
        "refresh_token" => refresh(&state, &caller, &form).await,
        "client_credentials" => client_credentials(&state, &caller, &form).await,
        _ => Err(OauthError::new(
            Code::UnsupportedGrantType,
            "this deployment issues tokens for authorization_code, refresh_token \
             and client_credentials",
        )),
    };

    match granted {
        Ok(issued) => {
            state.oidc_limits.forgive(&caller.id);
            reply(&issued)
        }
        Err(error) => {
            state
                .oidc_limits
                .failed(&caller.id, state.store.clock().now());
            error.direct()
        }
    }
}

async fn authorization_code(
    state: &AppState,
    caller: &Caller,
    form: &Request,
) -> Result<Issued, OauthError> {
    let (Some(code), Some(redirect_uri)) = (&form.code, &form.redirect_uri) else {
        return Err(OauthError::new(
            Code::InvalidRequest,
            "authorization_code needs code and redirect_uri",
        ));
    };
    let args = ExchangeCode {
        client_id: caller.id.clone(),
        client_secret: caller.secret.clone(),
        assertion: caller.assertion.clone(),
        code: code.clone(),
        redirect_uri: redirect_uri.clone(),
        code_verifier: form.code_verifier.clone(),
    };
    finish(rn_kernel::cmd::exchange_code(&super::context(state, Principal::Anonymous), &args).await)
}

async fn refresh(state: &AppState, caller: &Caller, form: &Request) -> Result<Issued, OauthError> {
    let Some(refresh_token) = &form.refresh_token else {
        return Err(OauthError::new(
            Code::InvalidRequest,
            "refresh_token is required for that grant",
        ));
    };
    let args = RefreshToken {
        client_id: caller.id.clone(),
        client_secret: caller.secret.clone(),
        assertion: caller.assertion.clone(),
        refresh_token: refresh_token.clone(),
        scopes: scopes_of(form)?,
    };
    finish(rn_kernel::cmd::refresh_token(&super::context(state, Principal::Anonymous), &args).await)
}

async fn client_credentials(
    state: &AppState,
    caller: &Caller,
    form: &Request,
) -> Result<Issued, OauthError> {
    let args = ClientCredentials {
        client_id: caller.id.clone(),
        client_secret: caller.secret.clone(),
        assertion: caller.assertion.clone(),
        scopes: scopes_of(form)?.unwrap_or_default(),
    };
    finish(
        rn_kernel::cmd::client_credentials(&super::context(state, Principal::Anonymous), &args)
            .await,
    )
}

/// The `scope` parameter, when there is one.
fn scopes_of(form: &Request) -> Result<Option<Vec<Scope>>, OauthError> {
    match form
        .scope
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => Ok(None),
        Some(raw) => Scope::parse_list(raw).map(Some).ok_or_else(|| {
            OauthError::new(
                Code::InvalidScope,
                "that scope list contains a scope this deployment does not admit",
            )
        }),
    }
}

/// Whether the client could prove it is itself, for the status alone.
async fn authenticated(state: &AppState, caller: &Caller) -> Result<(), OauthError> {
    use rn_kernel::oidc::{Credential, client as registry};
    let refuse = || {
        OauthError::new(
            Code::InvalidClient,
            "that client could not be authenticated",
        )
    };
    let reads = state.store.reads();
    let id = registry::decode_client_id(state.ids(), &caller.id).map_err(|_| refuse())?;
    let client = registry::load(&reads, id)
        .await
        .map_err(|_| refuse())?
        .ok_or_else(refuse)?;
    registry::authenticate(
        &reads,
        &client,
        &Credential {
            secret: caller.secret.as_deref(),
            assertion: caller.assertion.as_deref(),
        },
        &state.provider.token_endpoint(),
        state.store.clock().now(),
    )
    .await
    .map_err(|_| refuse())
}

/// A kernel outcome, as the specification's answer.
///
/// Every refusal is `invalid_grant` with one sentence, which is the point: a
/// code that never existed, a code that is spent, a code belonging to another
/// client, a wrong PKCE verifier and a wrong redirect URI must not be
/// distinguishable, because the difference is what a caller is measuring.
/// Client authentication is the one exception, and it is a `401` — a caller
/// that could not prove it is itself has to be told so, or it will retry
/// forever.
fn finish(outcome: rn_kernel::Outcome<Granted>) -> Result<Issued, OauthError> {
    match outcome {
        Ok(granted) => Ok(granted.issued),
        Err(error) => {
            tracing::debug!(%error, "a token request was refused");
            Err(OauthError::new(
                Code::InvalidGrant,
                "that grant is not one this deployment will honour",
            ))
        }
    }
}

/// The success body (RFC 6749 §5.1, OpenID Connect Core §3.1.3.3).
fn reply(issued: &Issued) -> Response {
    let Some(access) = &issued.access else {
        return OauthError::new(Code::ServerError, "no token was minted").direct();
    };
    let mut body = json!({
        "access_token": access.expose(),
        "token_type": "Bearer",
        "expires_in": issued.expires_in,
        "scope": issued.scopes,
    });
    let fields = body.as_object_mut().expect("an object");
    if let Some(refresh) = &issued.refresh {
        fields.insert("refresh_token".to_owned(), refresh.expose().into());
    }
    if let Some(id_token) = &issued.id_token {
        fields.insert("id_token".to_owned(), id_token.clone().into());
    }
    let mut response = Json(body).into_response();
    no_store(&mut response);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn with_basic(id: &str, secret: &str) -> HeaderMap {
        let raw = base64::engine::general_purpose::STANDARD.encode(format!("{id}:{secret}"));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Basic {raw}")).expect("a header"),
        );
        headers
    }

    #[test]
    fn a_basic_credential_is_read_and_form_decoded() {
        let headers = with_basic("c_one", "a+b%20c");
        let caller = caller(&headers, &Request::default()).expect("a caller");
        assert_eq!(caller.id, "c_one");
        assert_eq!(
            caller.secret.as_deref(),
            Some("a b c"),
            "RFC 6749 §2.3.1 form-decodes both halves"
        );
    }

    #[test]
    fn a_body_credential_is_read_when_there_is_no_header() {
        let form = Request {
            client_id: Some("c_one".into()),
            client_secret: Some("shh".into()),
            ..Request::default()
        };
        let caller = caller(&HeaderMap::new(), &form).expect("a caller");
        assert_eq!(caller.id, "c_one");
        assert_eq!(caller.secret.as_deref(), Some("shh"));
    }

    #[test]
    fn authenticating_twice_is_refused_rather_than_resolved() {
        let form = Request {
            client_secret: Some("shh".into()),
            ..Request::default()
        };
        let error = caller(&with_basic("c_one", "shh"), &form).expect_err("refused");
        assert_eq!(error.code, Code::InvalidRequest);
    }

    #[test]
    fn an_assertion_must_name_the_urn_the_specification_gives_it() {
        let form = Request {
            client_id: Some("c_one".into()),
            client_assertion: Some("ey.ey.sig".into()),
            client_assertion_type: Some("something-else".into()),
            ..Request::default()
        };
        assert_eq!(
            caller(&HeaderMap::new(), &form).expect_err("refused").code,
            Code::InvalidRequest
        );
        let form = Request {
            client_assertion_type: Some(ASSERTION_TYPE.into()),
            ..form
        };
        let caller = caller(&HeaderMap::new(), &form).expect("a caller");
        assert_eq!(caller.assertion.as_deref(), Some("ey.ey.sig"));
    }

    #[test]
    fn a_request_that_names_no_client_at_all_is_an_invalid_client() {
        assert_eq!(
            caller(&HeaderMap::new(), &Request::default())
                .expect_err("refused")
                .code,
            Code::InvalidClient
        );
    }

    #[test]
    fn form_decoding_survives_the_shapes_a_secret_can_take() {
        assert_eq!(form_decode("plain"), "plain");
        assert_eq!(form_decode("a+b"), "a b");
        assert_eq!(form_decode("%41%42"), "AB");
        assert_eq!(form_decode("100%"), "100%", "a trailing percent is itself");
        assert_eq!(form_decode("%zz"), "%zz", "and so is a bad escape");
    }
}
