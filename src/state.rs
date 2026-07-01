//! Application state shared across request handlers.

use std::sync::Arc;

/// State shared by every request handler.
///
/// It is cheap to clone (an `Arc` under the hood), so Axum can hand a copy to
/// each handler. Add shared resources — database pools, caches, config handles
/// — as fields on [`Inner`] rather than passing them around individually.
#[derive(Clone, Default)]
pub struct AppState {
    #[allow(dead_code)]
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    // e.g. db: PgPool, cache: Cache, ...
}

impl AppState {
    /// Builds the initial application state.
    pub fn new() -> Self {
        Self::default()
    }
}
