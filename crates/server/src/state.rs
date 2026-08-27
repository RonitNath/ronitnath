//! What every handler shares: the kernel, the database, the config, and this
//! release's identity.
//!
//! The kernel handles are built once, here, because there is exactly one of
//! each per process: one [`Store`] over the raft-replicated client, one
//! [`ClusterFeed`] pumping this node's listener, one warm principal cache and
//! one observation queue. A handler that built its own would be a second
//! change feed and a second cache that could disagree with the first.

use std::sync::Arc;

use hiqlite::Client;
use rn_kernel::feed::ClusterFeed;
use rn_kernel::ids::IdKey;
use rn_kernel::observe::{Matches, Observations};
use rn_kernel::principal::PrincipalCache;
use rn_kernel::store::{Clock, Store};

use crate::config::AppConfig;
use crate::sub::Connections;

/// How many resolved cookies stay warm. Two generations, so the real bound is
/// twice this (`kernel::principal::cache`).
const PRINCIPAL_CACHE: usize = 4_096;

#[derive(Clone)]
pub struct AppState {
    pub db: Client,
    pub config: Arc<AppConfig>,
    /// The revision this process is, stamped in at image build time.
    pub version: Arc<str>,
    /// Which replica this process is, from the provisioned `node.env`.
    pub node: Arc<str>,
    /// The kernel's view of the database. Commands write through it; query
    /// handlers get `store.reads()`, which has no `execute`.
    pub store: Arc<Store<Client>>,
    /// The change feed. Commands notify it; `/api/sub` subscribes to it.
    pub feed: Arc<ClusterFeed>,
    /// Resolved cookies, kept warm and invalidated from the feed.
    pub principals: Arc<PrincipalCache>,
    /// The observation lane: `last_seen`, drained on a timer, never on a read.
    pub observations: Arc<Observations>,
    /// The other half of that lane: identities whose proofs moved and are
    /// waiting to be scanned for match signals.
    pub matches: Arc<Matches>,
    /// Live subscription sockets, so the per-identity and per-node caps are
    /// one shared count rather than a number each connection believes.
    pub connections: Arc<Connections>,
}

impl AppState {
    /// # Panics
    ///
    /// If the configured `id_key` is not a well-formed AES-128 key. It cannot
    /// be: [`AppConfig::validate`](crate::config::AppConfig) refuses a
    /// malformed one at load, and dev mints a well-formed ephemeral one.
    #[must_use]
    pub fn new(db: Client, config: AppConfig) -> Self {
        Self::with_clock(db, config, Clock::System)
    }

    /// The same state on a clock the caller supplies.
    ///
    /// Everything time-dependent in the kernel reads `store.clock()`, so a
    /// fixed clock is how a test asks what the deployment looks like five
    /// minutes from now without waiting five minutes.
    ///
    /// # Panics
    ///
    /// As [`AppState::new`].
    #[must_use]
    pub fn with_clock(db: Client, config: AppConfig, clock: Clock) -> Self {
        let key = IdKey::from_hex(&config.require_id_key())
            .expect("the configuration refuses a malformed id key at load");
        let store = Arc::new(Store::new(db.clone(), key, clock));
        let feed = Arc::new(ClusterFeed::new(Arc::clone(&store)));
        let principals = Arc::new(PrincipalCache::new(PRINCIPAL_CACHE));
        // A revoked session must stop working, not merely stop being renewed.
        // The feed is the witness that says when (`sub::invalidate`).
        crate::sub::invalidate::spawn(Arc::clone(&feed), Arc::clone(&principals));
        Self {
            db,
            config: Arc::new(config),
            version: release_version().into(),
            node: node_name().into(),
            store,
            feed,
            principals,
            observations: Arc::new(Observations::new()),
            matches: Arc::new(Matches::new()),
            connections: Arc::new(Connections::default()),
        }
    }

    /// The key public ids are derived under.
    #[must_use]
    pub fn ids(&self) -> &IdKey {
        use rn_kernel::store::Reads as _;
        self.store.ids()
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
