//! Application state shared across request handlers.

use std::sync::Arc;
use std::time::Instant;

use crate::auth::oidc::OidcRegistry;
use crate::store::Store;

/// Auth knobs every request needs — split out from [`crate::config::Config`]
/// because that struct also carries things (bind address, body limits)
/// nothing in `src/auth` should have to know about.
#[derive(Clone)]
pub struct AuthConfig {
    /// Whether to set the `Secure` cookie flag (and use the `__Host-`
    /// prefix). Only true behind TLS — see [`crate::auth::session`].
    pub cookie_secure: bool,
    pub session_ttl_secs: i64,
    /// Whether `/signup` accepts new identities. `false` in deployments
    /// that provision accounts out-of-band.
    pub signup_open: bool,
}

/// State shared by every request handler.
///
/// It is cheap to clone (an `Arc` under the hood), so Axum can hand a copy to
/// each handler. Add shared resources — caches, config handles — as fields on
/// [`Inner`] rather than passing them around individually.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    started_at: Instant,
    auth: AuthConfig,
    oidc: OidcRegistry,
    public_url: String,
    owner_account_id: Option<i64>,
}

impl AppState {
    /// Builds the initial application state.
    pub fn new(store: Store, auth: AuthConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                started_at: Instant::now(),
                auth,
                oidc: OidcRegistry::empty(),
                public_url: "http://127.0.0.1:3130".into(),
                owner_account_id: None,
            }),
        }
    }

    pub fn with_oidc(self, oidc: OidcRegistry) -> Self {
        Self {
            inner: Arc::new(Inner {
                store: self.inner.store.clone(),
                started_at: self.inner.started_at,
                auth: self.inner.auth.clone(),
                oidc,
                public_url: self.inner.public_url.clone(),
                owner_account_id: self.inner.owner_account_id,
            }),
        }
    }

    pub fn with_public_url(self, public_url: String) -> Self {
        Self {
            inner: Arc::new(Inner {
                store: self.inner.store.clone(),
                started_at: self.inner.started_at,
                auth: self.inner.auth.clone(),
                oidc: self.inner.oidc.clone(),
                public_url,
                owner_account_id: self.inner.owner_account_id,
            }),
        }
    }

    pub fn with_owner_account_id(self, owner_account_id: Option<i64>) -> Self {
        Self {
            inner: Arc::new(Inner {
                store: self.inner.store.clone(),
                started_at: self.inner.started_at,
                auth: self.inner.auth.clone(),
                oidc: self.inner.oidc.clone(),
                public_url: self.inner.public_url.clone(),
                owner_account_id,
            }),
        }
    }

    pub fn owner_account_id(&self) -> Option<i64> {
        self.inner.owner_account_id
    }

    pub fn store(&self) -> &Store {
        &self.inner.store
    }

    pub fn auth_config(&self) -> AuthConfig {
        self.inner.auth.clone()
    }

    pub fn public_url(&self) -> &str {
        &self.inner.public_url
    }

    pub fn oidc(&self) -> &OidcRegistry {
        &self.inner.oidc
    }

    pub fn uptime(&self) -> std::time::Duration {
        self.inner.started_at.elapsed()
    }
}
