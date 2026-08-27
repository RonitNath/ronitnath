//! What every handler shares: the database, the config, and this release's identity.

use std::sync::Arc;

use hiqlite::Client;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub db: Client,
    pub config: Arc<AppConfig>,
    /// The revision this process is, stamped in at image build time.
    pub version: Arc<str>,
    /// Which replica this process is, from the provisioned `node.env`.
    pub node: Arc<str>,
}

impl AppState {
    #[must_use]
    pub fn new(db: Client, config: AppConfig) -> Self {
        Self {
            db,
            config: Arc::new(config),
            version: release_version().into(),
            node: node_name().into(),
        }
    }
}

/// Which revision this process is.
///
/// An `ARG` is scoped to the stage that declares it, so the Containerfile
/// re-declares `SOURCE_GIT_HASH` in the runtime stage. Without that every node
/// reports `dev`, which is the one question `/version` exists to answer.
#[must_use]
pub fn release_version() -> String {
    std::env::var("SOURCE_GIT_HASH")
        .ok()
        .filter(|hash| !hash.trim().is_empty())
        .unwrap_or_else(|| "dev".to_string())
}

/// Which replica this process is.
#[must_use]
pub fn node_name() -> String {
    std::env::var("RN_SITE_NODE")
        .ok()
        .filter(|node| !node.trim().is_empty())
        .unwrap_or_else(|| "local".to_string())
}
