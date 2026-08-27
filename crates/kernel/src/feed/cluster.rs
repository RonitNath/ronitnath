//! The feed as it runs on a cluster: hiqlite's listen/notify for the wake-ups.
//!
//! `notify` writes one small entry to the cache raft group, which every node
//! receives; a pump task turns those into a local broadcast so more than one
//! subscriber in a process can wait on them. hiqlite's `listen` is a single
//! consumer — calling it from two places would give each of them half the
//! wake-ups — so the pump is not an optimisation, it is the only correct way
//! to have two subscribers.
//!
//! This is the seam rung 6 replaces with Zenoh. Nothing durable rides here:
//! the offsets are hints, and everything that must survive a restart is in the
//! audit table.

use std::sync::Arc;

use futures_util::Stream;
use hiqlite::Client;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use super::Feed;
use crate::Offset;
use crate::error::Outcome;
use crate::event::Committed;
use crate::store::Store;

const WAKE_BACKLOG: usize = 64;

/// A change feed whose wake-ups cross nodes.
pub struct ClusterFeed {
    store: Arc<Store<Client>>,
    wake: broadcast::Sender<Offset>,
    pump: JoinHandle<()>,
}

impl ClusterFeed {
    /// Wrap a store and start pumping this node's listener into a broadcast.
    pub fn new(store: Arc<Store<Client>>) -> Self {
        let wake = broadcast::Sender::new(WAKE_BACKLOG);
        let pump = tokio::spawn(pump(store.engine().clone(), wake.clone()));
        Self { store, wake, pump }
    }

    /// The store it reads from.
    pub fn store(&self) -> &Arc<Store<Client>> {
        &self.store
    }
}

impl std::fmt::Debug for ClusterFeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClusterFeed")
    }
}

impl Drop for ClusterFeed {
    fn drop(&mut self) {
        self.pump.abort();
    }
}

async fn pump(client: Client, wake: broadcast::Sender<Offset>) {
    loop {
        match client.listen::<Offset>().await {
            Ok(offset) => {
                let _ = wake.send(offset);
            }
            Err(err) => {
                // The listener is a hint channel; losing it costs latency, so
                // it is worth saying once and worth retrying rather than
                // taking the process down.
                tracing::warn!(%err, "change feed listener stopped; retrying");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

impl Feed for ClusterFeed {
    async fn notify(&self, offset: Offset) {
        if let Err(err) = self.store.engine().notify(&offset).await {
            tracing::warn!(%err, offset, "change feed notification not sent");
        }
    }

    fn subscribe(&self) -> impl Stream<Item = Offset> + Send + 'static {
        super::local::wake_stream(self.wake.subscribe())
    }

    async fn read(&self, offset: Offset, limit: usize) -> Outcome<Vec<Committed>> {
        super::read_from(self.store.as_ref(), offset, limit).await
    }

    async fn head(&self) -> Outcome<Offset> {
        super::head_of(self.store.as_ref()).await
    }
}
