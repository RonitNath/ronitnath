//! Application state shared across request handlers.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::auth::oidc::OidcRegistry;
use crate::store::Store;

/// Auth knobs every request needs — split out from [`crate::config::Config`]
/// because that struct also carries things (bind address, body limits)
/// nothing in `src/auth` should have to know about it.
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
    photo_storage_dir: std::path::PathBuf,
    photo_max_pixels: u64,
    photo_max_side: u32,
    photo_ingest_semaphore: Arc<Semaphore>,
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
                photo_storage_dir: "data/photos".into(),
                photo_max_pixels: crate::photos::DEFAULT_MAX_IMAGE_PIXELS,
                photo_max_side: crate::photos::DEFAULT_MAX_IMAGE_SIDE,
                photo_ingest_semaphore: Arc::new(Semaphore::new(2)),
            }),
        }
    }

    pub fn with_oidc(self, oidc: OidcRegistry) -> Self {
        Self {
            inner: Arc::new(Inner {
                oidc,
                ..self.inner.as_ref().clone_inner()
            }),
        }
    }

    pub fn with_public_url(self, public_url: String) -> Self {
        Self {
            inner: Arc::new(Inner {
                public_url,
                ..self.inner.as_ref().clone_inner()
            }),
        }
    }

    pub fn with_owner_account_id(self, owner_account_id: Option<i64>) -> Self {
        Self {
            inner: Arc::new(Inner {
                owner_account_id,
                ..self.inner.as_ref().clone_inner()
            }),
        }
    }

    pub fn with_photo_storage_dir(self, photo_storage_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            inner: Arc::new(Inner {
                photo_storage_dir: photo_storage_dir.into(),
                ..self.inner.as_ref().clone_inner()
            }),
        }
    }

    pub fn with_photo_limits(self, max_pixels: u64, max_side: u32, concurrency: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                photo_max_pixels: max_pixels,
                photo_max_side: max_side,
                photo_ingest_semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
                ..self.inner.as_ref().clone_inner()
            }),
        }
    }

    pub fn photo_storage_dir(&self) -> &std::path::Path {
        &self.inner.photo_storage_dir
    }

    pub fn photo_max_pixels(&self) -> u64 {
        self.inner.photo_max_pixels
    }

    pub fn photo_max_side(&self) -> u32 {
        self.inner.photo_max_side
    }

    pub async fn photo_ingest_permit(&self) -> OwnedSemaphorePermit {
        self.inner
            .photo_ingest_semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("photo ingest semaphore closed")
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

impl Inner {
    fn clone_inner(&self) -> Self {
        Self {
            store: self.store.clone(),
            started_at: self.started_at,
            auth: self.auth.clone(),
            oidc: self.oidc.clone(),
            public_url: self.public_url.clone(),
            owner_account_id: self.owner_account_id,
            photo_storage_dir: self.photo_storage_dir.clone(),
            photo_max_pixels: self.photo_max_pixels,
            photo_max_side: self.photo_max_side,
            photo_ingest_semaphore: self.photo_ingest_semaphore.clone(),
        }
    }
}
