//! Live data: one WebSocket, one reconnect loop, and a reactive keyed row set
//! per subscribed query.
//!
//! The contract is resume-by-offset, not refetch (`docs/rebuild/plan.md`
//! §API): the client tells the server the last offset it applied and receives
//! only what it missed. Invalidate-and-refetch survives exactly once, as the
//! `resync` the server sends when a release changed what a query means.

mod backoff;
mod socket;
mod store;

use std::collections::BTreeMap;

use futures_util::{SinkExt, StreamExt, future};
use gloo_net::websocket::{Message, futures::WebSocket};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::de::DeserializeOwned;

use rn_api::{QueryRef, SubMessage, SubRequest};

pub use backoff::{Backoff, watchdog};
pub use store::{Applied, Refusal, Store};

/// A query's rows, live.
///
/// Cheap to copy and to hand to a component: it is two signals. Reading
/// [`Live::rows`] inside a view subscribes that view to every diff the server
/// sends for this query.
pub struct Live<T: Send + Sync + 'static> {
    store: RwSignal<Store<T>>,
    /// Bumped whenever a diff lands, so a `Memo` over the rows recomputes.
    revision: RwSignal<u64>,
}

impl<T: Send + Sync + 'static> Clone for Live<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Live<T> {}

impl<T: Clone + DeserializeOwned + Send + Sync + 'static> Live<T> {
    /// An empty, unconnected view of `query`.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            store: RwSignal::new(Store::new(query)),
            revision: RwSignal::new(0),
        }
    }

    /// Open the subscription and keep it open: connect, resume by offset,
    /// reconnect with full jitter, and treat a silent socket as a dead one.
    pub fn connect(self, params: &[(&str, &str)]) -> Self {
        let request = QueryRef {
            query: self.store.with_untracked(|s| s.query().to_owned()),
            params: params
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        };
        spawn_local(async move { run(self, request).await });
        self
    }

    /// Subscribe to `query` in one call.
    pub fn subscribe(query: impl Into<String>, params: &[(&str, &str)]) -> Self {
        Self::new(query).connect(params)
    }

    /// The rows in key order, as a reactive read.
    pub fn rows(&self) -> Vec<T> {
        self.entries().into_iter().map(|(_, row)| row).collect()
    }

    /// The rows with their keys, in key order, as a reactive read.
    pub fn entries(&self) -> Vec<(String, T)> {
        self.revision.track();
        self.store.with(|store| {
            store
                .rows()
                .iter()
                .map(|(key, row)| (key.clone(), row.clone()))
                .collect()
        })
    }

    /// One row by key, as a reactive read.
    pub fn get(&self, key: &str) -> Option<T> {
        self.revision.track();
        self.store.with(|store| store.rows().get(key).cloned())
    }

    /// The change-feed offset applied so far. Untracked: this is what the
    /// reconnect loop resumes from, not something a view renders.
    pub fn offset(&self) -> u64 {
        self.store.with_untracked(|store| store.offset())
    }

    /// Install the result of the initial query, before any diff arrives.
    pub fn seed(&self, rows: BTreeMap<String, T>, offset: u64) {
        self.store.update(|store| store.seed(rows, offset));
        self.revision.update(|r| *r += 1);
    }

    /// Apply one server message. Public so a test harness — or a caller
    /// driving the store from somewhere other than a socket — can.
    pub fn apply(&self, message: &SubMessage) -> Applied {
        let applied = self.store.try_update(|store| store.apply(message));
        match applied {
            Some(Applied::Changed) => {
                self.revision.update(|r| *r += 1);
                Applied::Changed
            }
            Some(Applied::Resynced) => {
                self.revision.update(|r| *r += 1);
                Applied::Resynced
            }
            Some(other) => other,
            None => Applied::Elsewhere,
        }
    }
}

/// The reconnect loop. It never returns: a bundle's subscription lives as long
/// as the page does.
async fn run<T: Clone + DeserializeOwned + Send + Sync + 'static>(
    live: Live<T>,
    request: QueryRef,
) {
    let mut backoff = Backoff::new();
    loop {
        if session(live, &request).await {
            backoff.succeeded();
        }
        socket::sleep(backoff.next_delay(socket::random())).await;
    }
}

/// One connection, from open to loss. Returns whether it ever carried a
/// message — a socket that opened and said nothing is a failure, and must not
/// reset the backoff.
async fn session<T: Clone + DeserializeOwned + Send + Sync + 'static>(
    live: Live<T>,
    request: &QueryRef,
) -> bool {
    let Ok(socket) = WebSocket::open(&socket::sub_url()) else {
        return false;
    };
    let (mut sink, mut stream) = socket.split();

    let hello = SubRequest {
        subscribe: vec![request.clone()],
        from: live.offset(),
    };
    let Ok(hello) = serde_json::to_string(&hello) else {
        return false;
    };
    if sink.send(Message::Text(hello)).await.is_err() {
        return false;
    }

    let mut heard = false;
    loop {
        let next = stream.next();
        let deadline = std::pin::pin!(socket::sleep(watchdog()));
        let message = match future::select(next, deadline).await {
            // Nothing for two heartbeats and change: the socket is a corpse.
            future::Either::Right(_) => return heard,
            future::Either::Left((Some(Ok(message)), _)) => message,
            future::Either::Left(_) => return heard,
        };
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(message) = serde_json::from_str::<SubMessage>(&text) else {
            continue;
        };
        heard = true;
        if live.apply(&message) == Applied::Resynced {
            // Our position is meaningless now; reopen from zero.
            return heard;
        }
    }
}
