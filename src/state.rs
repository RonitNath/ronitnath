//! Application state shared across request handlers.

use std::sync::Arc;
use std::time::Instant;

use crate::store::Store;

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
}

impl AppState {
    /// Builds the initial application state.
    pub fn new(store: Store) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                started_at: Instant::now(),
            }),
        }
    }

    pub fn store(&self) -> &Store {
        &self.inner.store
    }

    pub fn uptime(&self) -> std::time::Duration {
        self.inner.started_at.elapsed()
    }
}
