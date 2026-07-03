//! Application state shared across request handlers.

use std::sync::Arc;
use std::time::Instant;

use crate::store::Store;

/// Auth knobs every request needs — split out from [`crate::config::Config`]
/// because that struct also carries things (bind address, body limits)
/// nothing in `src/auth` should have to know about.
#[derive(Clone, Copy)]
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
}

impl AppState {
    /// Builds the initial application state.
    pub fn new(store: Store, auth: AuthConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                started_at: Instant::now(),
                auth,
            }),
        }
    }

    pub fn store(&self) -> &Store {
        &self.inner.store
    }

    pub fn auth_config(&self) -> AuthConfig {
        self.inner.auth
    }

    pub fn uptime(&self) -> std::time::Duration {
        self.inner.started_at.elapsed()
    }
}
