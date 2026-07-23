//! OIDC relying-party factor support.
//!
//! Providers are discovered from config at startup. Runtime flow state lives in
//! `pending_auth` and successful subjects become `factors.kind = 'oidc'` with
//! `external_id = "{issuer}#{sub}"`.

use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::http::{HeaderMap, Request, Response};
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AccessTokenHash, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet,
    EndpointNotSet, EndpointSet, IssuerUrl, OAuth2TokenResponse, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use serde::{Deserialize, Serialize};
use time::macros::format_description;

use crate::auth::login::{LoginOutcome, RequestContext};
use crate::auth::session;
use crate::error::AppError;
use crate::state::AppState;
use crate::store::factors::PendingOidcAuth;

#[derive(Debug, Clone, Serialize)]
pub struct OidcProviderButton {
    pub key: String,
    pub display_name: String,
}

#[derive(Clone, Deserialize)]
pub struct OidcProviderConfig {
    pub key: String,
    pub display_name: String,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Option<Vec<String>>,
    pub auto_provision: Option<bool>,
}

#[derive(Clone)]
pub struct OidcProvider {
    pub key: String,
    pub display_name: String,
    client_id: String,
    client_secret: String,
    scopes: Vec<String>,
    auto_provision: bool,
    metadata: CoreProviderMetadata,
}

#[derive(Clone)]
pub struct OidcRegistry {
    providers: Arc<HashMap<String, OidcProvider>>,
    http_client: DynOidcHttpClient,
}

impl OidcRegistry {
    pub fn empty() -> Self {
        Self {
            providers: Arc::new(HashMap::new()),
            http_client: DynOidcHttpClient::new(ReqwestOidcHttpClient::new()),
        }
    }

    pub async fn from_path(path: &str) -> anyhow::Result<Self> {
        Self::from_path_with_http(path, DynOidcHttpClient::new(ReqwestOidcHttpClient::new())).await
    }

    pub async fn from_path_with_http(
        path: &str,
        http_client: DynOidcHttpClient,
    ) -> anyhow::Result<Self> {
        let raw = match tokio::fs::read_to_string(path).await {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::empty_with_http(http_client));
            }
            Err(e) => return Err(e.into()),
        };
        if raw.trim().is_empty() {
            return Ok(Self::empty_with_http(http_client));
        }
        let configs: Vec<OidcProviderConfig> = serde_json::from_str(&raw)?;
        Self::from_configs(configs, http_client).await
    }

    pub async fn from_configs(
        configs: Vec<OidcProviderConfig>,
        http_client: DynOidcHttpClient,
    ) -> anyhow::Result<Self> {
        let mut providers = HashMap::new();
        for config in configs {
            let issuer = IssuerUrl::new(config.issuer_url.clone())?;
            let metadata = CoreProviderMetadata::discover_async(issuer, &http_client).await?;
            let provider = OidcProvider {
                key: config.key,
                display_name: config.display_name,
                client_id: config.client_id,
                client_secret: config.client_secret,
                scopes: normalize_scopes(config.scopes),
                auto_provision: config.auto_provision.unwrap_or(true),
                metadata,
            };
            providers.insert(provider.key.clone(), provider);
        }
        Ok(Self {
            providers: Arc::new(providers),
            http_client,
        })
    }

    fn empty_with_http(http_client: DynOidcHttpClient) -> Self {
        Self {
            providers: Arc::new(HashMap::new()),
            http_client,
        }
    }

    pub fn buttons(&self) -> Vec<OidcProviderButton> {
        let mut buttons = self
            .providers
            .values()
            .map(|provider| OidcProviderButton {
                key: provider.key.clone(),
                display_name: provider.display_name.clone(),
            })
            .collect::<Vec<_>>();
        buttons.sort_by(|a, b| a.display_name.cmp(&b.display_name).then(a.key.cmp(&b.key)));
        buttons
    }

    pub fn get(&self, key: &str) -> Option<&OidcProvider> {
        self.providers.get(key)
    }

    fn http_client(&self) -> &DynOidcHttpClient {
        &self.http_client
    }
}

fn normalize_scopes(scopes: Option<Vec<String>>) -> Vec<String> {
    let mut scopes =
        scopes.unwrap_or_else(|| vec!["openid".into(), "profile".into(), "email".into()]);
    if !scopes.iter().any(|scope| scope == "openid") {
        scopes.insert(0, "openid".into());
    }
    scopes
}

#[derive(Clone)]
pub struct DynOidcHttpClient(Arc<dyn OidcHttpClient>);

impl DynOidcHttpClient {
    pub fn new(client: impl OidcHttpClient + 'static) -> Self {
        Self(Arc::new(client))
    }
}

impl<'c> openidconnect::AsyncHttpClient<'c> for DynOidcHttpClient {
    type Error = OidcHttpError;
    type Future = Pin<Box<dyn Future<Output = Result<Response<Vec<u8>>, Self::Error>> + Send + 'c>>;

    fn call(&'c self, request: Request<Vec<u8>>) -> Self::Future {
        self.0.call(request)
    }
}

pub trait OidcHttpClient: Send + Sync {
    fn call<'c>(
        &'c self,
        request: Request<Vec<u8>>,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Vec<u8>>, OidcHttpError>> + Send + 'c>>;
}

#[derive(Clone)]
struct ReqwestOidcHttpClient {
    client: openidconnect::reqwest::Client,
}

impl ReqwestOidcHttpClient {
    fn new() -> Self {
        let client = openidconnect::reqwest::ClientBuilder::new()
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client should build");
        Self { client }
    }
}

impl OidcHttpClient for ReqwestOidcHttpClient {
    fn call<'c>(
        &'c self,
        request: Request<Vec<u8>>,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Vec<u8>>, OidcHttpError>> + Send + 'c>> {
        Box::pin(async move {
            let request: openidconnect::reqwest::Request = request
                .try_into()
                .map_err(|e: openidconnect::reqwest::Error| OidcHttpError(e.to_string()))?;
            let response = self
                .client
                .execute(request)
                .await
                .map_err(|e| OidcHttpError(e.to_string()))?;
            let status = response.status();
            let headers = response.headers().clone();
            let body = response
                .bytes()
                .await
                .map_err(|e| OidcHttpError(e.to_string()))?
                .to_vec();
            let mut builder = Response::builder().status(status);
            *builder.headers_mut().expect("response builder headers") = headers;
            builder.body(body).map_err(|e| OidcHttpError(e.to_string()))
        })
    }
}

#[derive(Debug, Clone)]
pub struct OidcHttpError(String);

impl Display for OidcHttpError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for OidcHttpError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OidcIntent {
    Login,
    Link,
    GuestLogin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestOidcClaim {
    pub event_link_id: i64,
    pub owner_account_id: i64,
    pub person_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOidcState {
    pub provider_key: String,
    pub nonce: String,
    pub pkce_verifier: String,
    pub redirect_uri: String,
    pub intent: OidcIntent,
    pub next: Option<String>,
    #[serde(default)]
    pub guest_claim: Option<GuestOidcClaim>,
}

pub async fn start(
    state: &AppState,
    provider_key: &str,
    redirect_uri: String,
    intent: OidcIntent,
    identity_id: Option<i64>,
    account_id: Option<i64>,
    next: Option<String>,
    guest_claim: Option<GuestOidcClaim>,
) -> Result<String, AppError> {
    let provider = state.oidc().get(provider_key).ok_or(AppError::NotFound)?;
    let client = provider.client(&redirect_uri)?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let mut authorize = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            openidconnect::Nonce::new_random,
        )
        .set_pkce_challenge(pkce_challenge);
    for scope in &provider.scopes {
        authorize = authorize.add_scope(Scope::new(scope.clone()));
    }
    let (auth_url, csrf_state, nonce) = authorize.url();
    let pending = PendingOidcState {
        provider_key: provider.key.clone(),
        nonce: nonce.secret().clone(),
        pkce_verifier: pkce_verifier.secret().clone(),
        redirect_uri,
        intent,
        next,
        guest_claim,
    };
    state
        .store()
        .create_pending_oidc(csrf_state.secret(), identity_id, account_id, &pending)
        .await?;
    Ok(auth_url.to_string())
}

pub async fn callback(
    state: &AppState,
    route_provider_key: &str,
    code: String,
    csrf_state: String,
    ctx: RequestContext<'_>,
) -> Result<(LoginOutcome, Option<String>), AppError> {
    let pending = state
        .store()
        .consume_pending_oidc(&csrf_state)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => {
                AppError::InvalidCredentials("OIDC state was missing or expired".into())
            }
            other => AppError::from(other),
        })?;
    if pending.state.provider_key != route_provider_key {
        return Err(AppError::InvalidCredentials(
            "OIDC state did not match this provider".into(),
        ));
    }
    let provider = state
        .oidc()
        .get(&pending.state.provider_key)
        .ok_or(AppError::NotFound)?;
    let client = provider.client(&pending.state.redirect_uri)?;
    let token_response = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|e| AppError::Other(anyhow::anyhow!("OIDC token endpoint unavailable: {e}")))?
        .set_pkce_verifier(PkceCodeVerifier::new(pending.state.pkce_verifier.clone()))
        .request_async(state.oidc().http_client())
        .await
        .map_err(|e| AppError::InvalidCredentials(format!("OIDC token exchange failed: {e}")))?;

    let id_token = token_response.id_token().ok_or_else(|| {
        AppError::InvalidCredentials("OIDC provider did not return an ID token".into())
    })?;
    let verifier = client.id_token_verifier();
    let nonce = openidconnect::Nonce::new(pending.state.nonce.clone());
    let claims = id_token
        .claims(&verifier, &nonce)
        .map_err(|e| AppError::InvalidCredentials(format!("OIDC ID token rejected: {e}")))?;
    if let Some(expected_hash) = claims.access_token_hash() {
        let actual_hash = AccessTokenHash::from_token(
            token_response.access_token(),
            id_token.signing_alg().map_err(|e| {
                AppError::InvalidCredentials(format!("OIDC ID token alg rejected: {e}"))
            })?,
            id_token.signing_key(&verifier).map_err(|e| {
                AppError::InvalidCredentials(format!("OIDC ID token key rejected: {e}"))
            })?,
        )
        .map_err(|e| {
            AppError::InvalidCredentials(format!("OIDC access-token hash rejected: {e}"))
        })?;
        if actual_hash != *expected_hash {
            return Err(AppError::InvalidCredentials(
                "OIDC access-token hash mismatch".into(),
            ));
        }
    }

    let external_id = format!("{}#{}", claims.issuer().as_str(), claims.subject().as_str());
    let email = claims.email().map(|email| email.as_str().to_string());
    let display_name = claims
        .name()
        .and_then(|name| name.get(None))
        .map(|name| name.as_str().to_string())
        .or_else(|| email.clone())
        .unwrap_or_else(|| claims.subject().as_str().to_string());

    let request_id = ctx.request_id;
    let (identity_id, account_id, factor_id, outcome) = match pending.state.intent {
        OidcIntent::Login => {
            let (identity_id, account_id, factor_id) = login_or_provision(
                state,
                provider,
                &external_id,
                &display_name,
                email.as_deref(),
            )
            .await?;
            let outcome = issue_session(state, identity_id, account_id, ctx).await?;
            (identity_id, account_id, factor_id, outcome)
        }
        OidcIntent::Link => {
            let (identity_id, account_id, factor_id) =
                link_factor(state, &pending, &external_id, email.as_deref()).await?;
            let outcome = issue_session(state, identity_id, account_id, ctx).await?;
            (identity_id, account_id, factor_id, outcome)
        }
        OidcIntent::GuestLogin => {
            guest_login_or_claim(
                state,
                provider,
                &pending,
                &external_id,
                email.as_deref(),
                ctx,
            )
            .await?
        }
    };
    state.store().touch_factor_last_used(factor_id).await?;
    state
        .store()
        .audit(
            Some(identity_id),
            Some(account_id),
            request_id,
            "login.succeeded",
            "identity",
            Some(&identity_id.to_string()),
            &serde_json::json!({ "factor": "oidc", "provider": provider.key }),
        )
        .await?;
    Ok((outcome, pending.state.next))
}

async fn guest_login_or_claim(
    state: &AppState,
    provider: &OidcProvider,
    pending: &PendingOidcAuth,
    external_id: &str,
    email: Option<&str>,
    ctx: RequestContext<'_>,
) -> Result<(i64, i64, i64, LoginOutcome), AppError> {
    if let Some(factor) = state
        .store()
        .find_factor_by_external("oidc", external_id)
        .await?
    {
        let binding = state
            .store()
            .active_guest_binding(factor.identity_id)
            .await?
            .filter(|binding| Some(binding.owner_account_id) == state.owner_account_id())
            .ok_or_else(|| {
                AppError::Forbidden("this ronitnath ID is not linked to an invited guest".into())
            })?;
        let outcome =
            issue_session(state, factor.identity_id, binding.guest_account_id, ctx).await?;
        return Ok((
            factor.identity_id,
            binding.guest_account_id,
            factor.id,
            outcome,
        ));
    }

    let claim = pending.state.guest_claim.as_ref().ok_or_else(|| {
        AppError::Forbidden(
            "use the claim link from a personal invitation before signing in for the first time"
                .into(),
        )
    })?;
    if Some(claim.owner_account_id) != state.owner_account_id() {
        return Err(AppError::Forbidden(
            "this invitation does not belong to this site".into(),
        ));
    }
    let live_link = state
        .store()
        .find_live_event_link_by_id(claim.event_link_id)
        .await?
        .filter(|link| {
            link.account_id == claim.owner_account_id && link.person_id == Some(claim.person_id)
        })
        .ok_or(AppError::NotFound)?;
    let person = state
        .store()
        .find_person(claim.owner_account_id, claim.person_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let material = prepare_session(state)?;
    let metadata = serde_json::json!({ "email": email });
    let (identity_id, guest_account_id, factor_id) = state
        .store()
        .claim_guest_with_oidc(
            claim.owner_account_id,
            claim.person_id,
            live_link.id,
            &person.name,
            external_id,
            &metadata,
            &material.token_hash,
            &material.csrf_token,
            &material.expires_at,
            ctx.user_agent,
            ctx.ip,
        )
        .await
        .map_err(|error| match &error {
            sqlx::Error::RowNotFound => AppError::NotFound,
            sqlx::Error::Database(db) if db.is_unique_violation() => AppError::Forbidden(
                "this invitation or ronitnath ID has already been claimed".into(),
            ),
            _ => AppError::from(error),
        })?;
    state
        .store()
        .audit(
            Some(identity_id),
            Some(claim.owner_account_id),
            ctx.request_id,
            "guest.claimed",
            "person",
            Some(&claim.person_id.to_string()),
            &serde_json::json!({
                "guest_account_id": guest_account_id,
                "event_link_id": live_link.id,
                "factor": "oidc",
                "provider": provider.key,
            }),
        )
        .await?;
    Ok((identity_id, guest_account_id, factor_id, material.outcome))
}

async fn login_or_provision(
    state: &AppState,
    provider: &OidcProvider,
    external_id: &str,
    display_name: &str,
    email: Option<&str>,
) -> Result<(i64, i64, i64), AppError> {
    if let Some(factor) = state
        .store()
        .find_factor_by_external("oidc", external_id)
        .await?
    {
        let membership = state
            .store()
            .find_primary_membership(factor.identity_id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "identity {} has an oidc factor but no membership",
                    factor.identity_id
                )
            })?;
        return Ok((factor.identity_id, membership.account_id, factor.id));
    }
    if !provider.auto_provision {
        return Err(AppError::Forbidden(
            "this SSO provider is not open for new accounts".into(),
        ));
    }
    let (identity_id, account_id, factor_id) = state
        .store()
        .signup_with_oidc(display_name, external_id, email)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                AppError::Invalid("that SSO identity is already linked".into())
            }
            _ => AppError::from(e),
        })?;
    state
        .store()
        .audit(
            Some(identity_id),
            Some(account_id),
            None,
            "identity.signed_up",
            "identity",
            Some(&identity_id.to_string()),
            &serde_json::json!({ "factor": "oidc" }),
        )
        .await?;
    Ok((identity_id, account_id, factor_id))
}

async fn link_factor(
    state: &AppState,
    pending: &PendingOidcAuth,
    external_id: &str,
    email: Option<&str>,
) -> Result<(i64, i64, i64), AppError> {
    let identity_id = pending.identity_id.ok_or_else(|| {
        AppError::InvalidCredentials("OIDC link flow lost its session binding".into())
    })?;
    let account_id = pending.account_id.ok_or_else(|| {
        AppError::InvalidCredentials("OIDC link flow lost its account binding".into())
    })?;
    if let Some(existing) = state
        .store()
        .find_factor_by_external("oidc", external_id)
        .await?
    {
        let message = if existing.identity_id == identity_id {
            "that SSO identity is already linked"
        } else {
            "that SSO identity belongs to another account"
        };
        return Err(AppError::Invalid(message.into()));
    }
    let metadata = serde_json::json!({ "email": email });
    let factor = state
        .store()
        .create_oidc_factor(identity_id, external_id, &metadata)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                AppError::Invalid("that SSO identity is already linked".into())
            }
            _ => AppError::from(e),
        })?;
    state
        .store()
        .audit(
            Some(identity_id),
            Some(account_id),
            None,
            "factor.linked",
            "factor",
            Some(&factor.id.to_string()),
            &serde_json::json!({ "kind": "oidc" }),
        )
        .await?;
    Ok((identity_id, account_id, factor.id))
}

struct SessionMaterial {
    outcome: LoginOutcome,
    token_hash: String,
    csrf_token: String,
    expires_at: String,
}

fn prepare_session(state: &AppState) -> Result<SessionMaterial, AppError> {
    let ttl_secs = state.auth_config().session_ttl_secs;
    let raw_token = session::generate_token();
    let token_hash = session::hash_token(&raw_token);
    let csrf_token = session::generate_token();
    let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    let expires_at = (time::OffsetDateTime::now_utc() + time::Duration::seconds(ttl_secs))
        .format(&format)
        .map_err(|e| anyhow::anyhow!("formatting session expiry: {e}"))?;
    Ok(SessionMaterial {
        outcome: LoginOutcome {
            raw_token,
            ttl_secs,
        },
        token_hash,
        csrf_token,
        expires_at,
    })
}

async fn issue_session(
    state: &AppState,
    identity_id: i64,
    account_id: i64,
    ctx: RequestContext<'_>,
) -> Result<LoginOutcome, AppError> {
    let material = prepare_session(state)?;
    state
        .store()
        .create_session(
            identity_id,
            account_id,
            &material.token_hash,
            &material.csrf_token,
            &material.expires_at,
            ctx.user_agent,
            ctx.ip,
        )
        .await?;
    Ok(material.outcome)
}

impl OidcProvider {
    fn client(
        &self,
        redirect_uri: &str,
    ) -> Result<
        CoreClient<
            EndpointSet,
            EndpointNotSet,
            EndpointNotSet,
            EndpointNotSet,
            EndpointMaybeSet,
            EndpointMaybeSet,
        >,
        AppError,
    > {
        let redirect_uri = RedirectUrl::new(redirect_uri.to_string())
            .map_err(|e| AppError::Other(anyhow::anyhow!("invalid OIDC redirect URL: {e}")))?;
        Ok(CoreClient::from_provider_metadata(
            self.metadata.clone(),
            ClientId::new(self.client_id.clone()),
            Some(ClientSecret::new(self.client_secret.clone())),
        )
        .set_redirect_uri(redirect_uri))
    }
}

pub fn canonical_redirect_uri(public_url: &str, provider_key: &str) -> Result<String, AppError> {
    let mut base = openidconnect::url::Url::parse(public_url)
        .map_err(|e| AppError::Other(anyhow::anyhow!("invalid public URL: {e}")))?;
    base.set_path(&format!("/auth/oidc/{provider_key}/callback"));
    base.set_query(None);
    base.set_fragment(None);
    Ok(base.to_string())
}

pub fn redirect_uri(headers: &HeaderMap, provider_key: &str) -> Result<String, AppError> {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost");
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("http");
    Ok(format!(
        "{proto}://{host}/auth/oidc/{provider_key}/callback"
    ))
}
