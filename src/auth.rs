use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use axum::Router;
use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::sync::RwLock;
use tower_sessions::cookie::{Key, SameSite};
use tower_sessions::session_store::ExpiredDeletion as _;
use tower_sessions::{Expiry, Session, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;
use uuid::Uuid;

const SESSION_IDENTITY_KEY: &str = "identity_id";
const SESSION_ACCOUNT_KEY: &str = "account_id";
const SESSION_ROLE_KEY: &str = "role";
const SESSION_NONCE_KEY: &str = "oidc_nonce";
const SESSION_STATE_KEY: &str = "oidc_state";
const SESSION_RETURN_TO_KEY: &str = "oidc_return_to";
const SESSION_PKCE_VERIFIER_KEY: &str = "oidc_pkce_verifier";
const JWKS_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub(crate) struct OidcConfig {
    pub(crate) issuer: String,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
    pub(crate) redirect_uri: String,
    pub(crate) post_login_redirect: String,
    pub(crate) post_logout_redirect: String,
    pub(crate) expected_audience: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionConfig {
    pub(crate) table_name: String,
    pub(crate) cookie_name: String,
    pub(crate) cookie_secure: bool,
    pub(crate) expiry_hours: i64,
    pub(crate) cookie_domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionData {
    pub(crate) identity_id: Uuid,
    pub(crate) account_id: Option<Uuid>,
    pub(crate) role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdClaims {
    iss: String,
    sub: Uuid,
    aud: String,
    exp: i64,
    iat: i64,
    auth_time: i64,
    #[serde(default)]
    nonce: Option<String>,
    sid: Uuid,
    aid: Option<Uuid>,
    role: String,
    #[serde(default)]
    isoastra_role: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kty: String,
    crv: String,
    kid: String,
    x: String,
    y: String,
}

#[derive(Debug, Deserialize)]
struct JwksDocument {
    keys: Vec<Jwk>,
}

pub(crate) struct JwksClient {
    issuer: String,
    http: reqwest::Client,
    cache: RwLock<HashMap<String, DecodingKey>>,
}

impl JwksClient {
    pub(crate) async fn new(issuer: impl Into<String>) -> anyhow::Result<Arc<Self>> {
        let client = Arc::new(Self {
            issuer: issuer.into(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .context("build reqwest client")?,
            cache: RwLock::new(HashMap::new()),
        });
        client.refresh().await?;
        let refresh_handle = Arc::clone(&client);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(JWKS_REFRESH_INTERVAL);
            tick.tick().await;
            loop {
                tick.tick().await;
                if let Err(err) = refresh_handle.refresh().await {
                    tracing::warn!(error = %err, "jwks refresh failed");
                }
            }
        });
        Ok(client)
    }

    async fn refresh(&self) -> anyhow::Result<()> {
        let url = format!("{}/jwks", self.issuer);
        let doc: JwksDocument = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("fetch {url}"))?
            .error_for_status()
            .with_context(|| format!("non-success from {url}"))?
            .json()
            .await
            .context("parse jwks document")?;

        let mut new_cache = HashMap::new();
        for jwk in doc.keys {
            if jwk.kty != "EC" || jwk.crv != "P-256" {
                tracing::warn!(kid = %jwk.kid, kty = %jwk.kty, "skipping non-P-256 jwk");
                continue;
            }
            let x = URL_SAFE_NO_PAD
                .decode(jwk.x.as_bytes())
                .with_context(|| format!("invalid x on jwk {}", jwk.kid))?;
            let y = URL_SAFE_NO_PAD
                .decode(jwk.y.as_bytes())
                .with_context(|| format!("invalid y on jwk {}", jwk.kid))?;
            let key = DecodingKey::from_ec_components(
                &URL_SAFE_NO_PAD.encode(&x),
                &URL_SAFE_NO_PAD.encode(&y),
            )
            .with_context(|| format!("build decoding key for {}", jwk.kid))?;
            new_cache.insert(jwk.kid, key);
        }

        let count = new_cache.len();
        *self.cache.write().await = new_cache;
        tracing::info!(keys = count, issuer = %self.issuer, "jwks refreshed");
        Ok(())
    }

    async fn verify_id_token(
        &self,
        token: &str,
        issuer: &str,
        audience: &str,
        expected_nonce: Option<&str>,
    ) -> anyhow::Result<IdClaims> {
        let header = jsonwebtoken::decode_header(token).context("invalid jwt header")?;
        let kid = header.kid.context("id_token missing kid")?;

        let key = {
            let cache = self.cache.read().await;
            cache.get(&kid).cloned()
        };
        let key = if let Some(key) = key {
            key
        } else {
            drop(self.refresh().await);
            let cache = self.cache.read().await;
            cache
                .get(&kid)
                .cloned()
                .with_context(|| format!("no jwk for kid={kid} after refresh"))?
        };

        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_issuer(&[issuer]);
        validation.set_audience(&[audience]);
        validation.validate_exp = true;
        validation.validate_aud = true;
        validation.leeway = 60;

        let data =
            jsonwebtoken::decode::<IdClaims>(token, &key, &validation).context("jwt verify")?;
        if let Some(expected) = expected_nonce {
            match &data.claims.nonce {
                Some(got) if got == expected => {}
                Some(got) => anyhow::bail!("nonce mismatch: got {got}, expected {expected}"),
                None => anyhow::bail!("id_token missing nonce"),
            }
        }
        Ok(data.claims)
    }
}

pub(crate) async fn initialize_session_store(
    pool: sqlx::SqlitePool,
    config: &SessionConfig,
) -> anyhow::Result<SqliteStore> {
    let store = SqliteStore::new(pool)
        .with_table_name(&config.table_name)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    store
        .migrate()
        .await
        .map_err(|err| anyhow::anyhow!("session store migration failed: {err}"))?;
    let cleanup = store.clone();
    tokio::spawn(async move {
        if let Err(err) = cleanup
            .continuously_delete_expired(tokio::time::Duration::from_secs(3600))
            .await
        {
            tracing::warn!(error = %err, "session cleanup stopped");
        }
    });
    Ok(store)
}

pub(crate) fn session_layer(
    store: SqliteStore,
    cookie_key: Key,
    config: &SessionConfig,
) -> SessionManagerLayer<SqliteStore, tower_sessions::service::PrivateCookie> {
    let mut layer = SessionManagerLayer::new(store)
        .with_name(config.cookie_name.clone())
        .with_same_site(SameSite::Lax)
        .with_secure(config.cookie_secure)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(
            config.expiry_hours,
        )))
        .with_always_save(true)
        .with_private(cookie_key);
    if let Some(domain) = &config.cookie_domain {
        layer = layer.with_domain(domain.clone());
    }
    layer
}

#[derive(Clone)]
struct OidcState {
    config: OidcConfig,
    http_client: reqwest::Client,
    jwks: Arc<JwksClient>,
}

pub(crate) fn oidc_router(config: OidcConfig, jwks: Arc<JwksClient>) -> Router {
    let state = OidcState {
        config,
        http_client: reqwest::ClientBuilder::new()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new()),
        jwks,
    };
    Router::new()
        .route("/auth/login", get(oidc_login))
        .route("/auth/callback", get(oidc_callback))
        .route("/auth/logout", get(oidc_logout))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct LoginQuery {
    return_to: Option<String>,
}

async fn oidc_login(
    State(state): State<OidcState>,
    session: Session,
    Query(params): Query<LoginQuery>,
) -> Redirect {
    let nonce = Uuid::new_v4().to_string();
    let csrf_state = Uuid::new_v4().to_string();
    let verifier = random_verifier();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let _ = session.insert(SESSION_NONCE_KEY, &nonce).await;
    let _ = session.insert(SESSION_STATE_KEY, &csrf_state).await;
    let _ = session.insert(SESSION_PKCE_VERIFIER_KEY, &verifier).await;
    if let Some(ref return_to) = params.return_to {
        let _ = session.insert(SESSION_RETURN_TO_KEY, return_to).await;
    }

    let auth_url = format!(
        "{issuer}/authorize?client_id={cid}&redirect_uri={ru}&response_type=code&scope=openid%20profile%20email&state={st}&nonce={n}&code_challenge={cc}&code_challenge_method=S256",
        issuer = state.config.issuer,
        cid = urlencoding::encode(&state.config.client_id),
        ru = urlencoding::encode(&state.config.redirect_uri),
        st = urlencoding::encode(&csrf_state),
        n = urlencoding::encode(&nonce),
        cc = urlencoding::encode(&challenge),
    );
    Redirect::temporary(&auth_url)
}

fn random_verifier() -> String {
    let mut buf = [0_u8; 48];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: String,
    state: Option<String>,
}

async fn oidc_callback(
    State(state): State<OidcState>,
    session: Session,
    Query(params): Query<CallbackQuery>,
) -> Result<Response, StatusCode> {
    let expected_state: Option<String> = session.get(SESSION_STATE_KEY).await.ok().flatten();
    let expected_nonce: Option<String> = session.get(SESSION_NONCE_KEY).await.ok().flatten();
    let pkce_verifier: Option<String> = session.get(SESSION_PKCE_VERIFIER_KEY).await.ok().flatten();
    session.remove::<String>(SESSION_STATE_KEY).await.ok();
    session.remove::<String>(SESSION_NONCE_KEY).await.ok();
    session
        .remove::<String>(SESSION_PKCE_VERIFIER_KEY)
        .await
        .ok();

    if expected_state.as_deref() != params.state.as_deref() {
        tracing::warn!("oauth state mismatch");
        return Err(StatusCode::BAD_REQUEST);
    }
    let Some(verifier) = pkce_verifier else {
        tracing::warn!("pkce verifier missing from session");
        return Err(StatusCode::BAD_REQUEST);
    };

    let token_url = format!("{}/token", state.config.issuer);
    let resp = state
        .http_client
        .post(&token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &params.code),
            ("redirect_uri", &state.config.redirect_uri),
            ("client_id", &state.config.client_id),
            ("client_secret", &state.config.client_secret),
            ("code_verifier", &verifier),
        ])
        .send()
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, "token exchange failed");
            StatusCode::BAD_GATEWAY
        })?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!(body, "token exchange returned error");
        return Err(StatusCode::BAD_GATEWAY);
    }

    let token_resp: TokenResponse = resp.json().await.map_err(|err| {
        tracing::warn!(error = %err, "failed to parse token response");
        StatusCode::BAD_GATEWAY
    })?;

    let claims = state
        .jwks
        .verify_id_token(
            &token_resp.id_token,
            &state.config.issuer,
            &state.config.expected_audience,
            expected_nonce.as_deref(),
        )
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, "id_token verification failed");
            StatusCode::BAD_GATEWAY
        })?;

    let data = SessionData {
        identity_id: claims.sub,
        account_id: claims.aid,
        role: claims.isoastra_role.or(Some(claims.role)),
    };
    set_session_data(&session, &data).await.map_err(|err| {
        tracing::warn!(error = %err, "persist session");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(access) = token_resp.access_token {
        session.insert("access_token", access).await.ok();
    }

    let return_to: Option<String> = session.get(SESSION_RETURN_TO_KEY).await.ok().flatten();
    session.remove::<String>(SESSION_RETURN_TO_KEY).await.ok();
    let target = return_to.unwrap_or_else(|| state.config.post_login_redirect.clone());
    Ok(Redirect::temporary(&target).into_response())
}

async fn oidc_logout(State(state): State<OidcState>, session: Session) -> impl IntoResponse {
    clear_session(&session).await;
    let end_session_url = format!(
        "{}/end_session?client_id={}&post_logout_redirect_uri={}",
        state.config.issuer,
        urlencoding::encode(&state.config.client_id),
        urlencoding::encode(&state.config.post_logout_redirect),
    );
    Redirect::temporary(&end_session_url)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: String,
    #[serde(default)]
    access_token: Option<String>,
}

async fn get_session_data(session: &Session) -> Option<SessionData> {
    let identity_id: Uuid = session.get(SESSION_IDENTITY_KEY).await.ok()??;
    let account_id: Option<Uuid> = session.get(SESSION_ACCOUNT_KEY).await.ok().flatten();
    let role: Option<String> = session.get(SESSION_ROLE_KEY).await.ok().flatten();
    Some(SessionData {
        identity_id,
        account_id,
        role,
    })
}

async fn set_session_data(session: &Session, data: &SessionData) -> anyhow::Result<()> {
    session
        .insert(SESSION_IDENTITY_KEY, data.identity_id)
        .await?;
    if let Some(account_id) = data.account_id {
        session.insert(SESSION_ACCOUNT_KEY, account_id).await?;
    }
    if let Some(role) = &data.role {
        session.insert(SESSION_ROLE_KEY, role).await?;
    }
    Ok(())
}

async fn clear_session(session: &Session) {
    session.flush().await.ok();
}

pub(crate) async fn optional_auth(session: Session, mut request: Request, next: Next) -> Response {
    if let Some(data) = get_session_data(&session).await {
        request.extensions_mut().insert(data);
    }
    next.run(request).await
}

pub(crate) fn cookie_key_from_secret(secret: &[u8]) -> Key {
    Key::derive_from(secret)
}
