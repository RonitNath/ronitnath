//! A feed whose wake-ups do not leave the process.
//!
//! Correct for a single node and for every test and benchmark, which is what
//! it is for. The durable half — the audit table — is identical to the
//! cluster's, so a test that reads the feed is reading the real thing; only
//! the notification hop is missing, and the integration test against a live
//! hiqlite node is where that gets proven instead.

use std::sync::Arc;

use futures_util::{Stream, stream};
use tokio::sync::broadcast;

use super::Feed;
use crate::Offset;
use crate::error::Outcome;
use crate::event::Committed;
use crate::store::{Sql, Store};

/// How many wake-ups a slow subscriber may fall behind before it is told to
/// catch up by reading. Small on purpose: the offsets are hints, and a
/// subscriber that missed some reads from its own cursor anyway.
const WAKE_BACKLOG: usize = 64;

/// A change feed backed by one process's audit table.
#[derive(Debug, Clone)]
pub struct LocalFeed<S: Sql> {
    store: Arc<Store<S>>,
    wake: broadcast::Sender<Offset>,
}

impl<S: Sql> LocalFeed<S> {
    /// Wrap a store.
    pub fn new(store: Arc<Store<S>>) -> Self {
        Self {
            store,
            wake: broadcast::Sender::new(WAKE_BACKLOG),
        }
    }

    /// The store it reads from, for a caller assembling a [`Ctx`].
    ///
    /// [`Ctx`]: crate::cmd::Ctx
    pub fn store(&self) -> &Arc<Store<S>> {
        &self.store
    }
}

impl<S: Sql> Feed for LocalFeed<S> {
    async fn notify(&self, offset: Offset) {
        // An error here means nobody is subscribed, which is not a problem.
        let _ = self.wake.send(offset);
    }

    fn subscribe(&self) -> impl Stream<Item = Offset> + Send + 'static {
        wake_stream(self.wake.subscribe())
    }

    async fn read(&self, offset: Offset, limit: usize) -> Outcome<Vec<Committed>> {
        super::read_from(self.store.as_ref(), offset, limit).await
    }
}

/// Turn a broadcast receiver into a stream, dropping the lag notices: falling
/// behind on hints is not an error, it is a subscriber that should read.
pub(crate) fn wake_stream(rx: broadcast::Receiver<Offset>) -> impl Stream<Item = Offset> + Send {
    stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(offset) => return Some((offset, rx)),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    })
}
