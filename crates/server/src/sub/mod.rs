//! `WS /api/sub` — the change feed, as row diffs, per subscribed query.
//!
//! The contract is resume-by-offset (`docs/rebuild/plan.md` §API). A client
//! names the queries it wants and the offset it has already applied; the
//! server reads the feed forward from there, asks each subscribed query which
//! of its keys the event moved, re-reads exactly those keys and sends the
//! difference. A client that has applied nothing (`from: 0`) is seeded with
//! the whole result set at the current head, so opening a socket is also how a
//! bundle gets its first rows.
//!
//! Four things can end or interrupt a subscription, and each has one answer:
//!
//! * the client falls behind the per-connection bound → [`SubMessage::Resync`]
//!   and the backlog is dropped ([`outbox`]);
//! * the client is running a different release → `Resync`, because a query's
//!   shape is part of the release and a diff across that boundary is a lie;
//! * the identity or the node is at its socket cap → close `1013`
//!   ([`limits`]);
//! * silence → a heartbeat every 25 s, so a client that stops seeing them
//!   knows the socket is a corpse rather than a quiet one.

pub mod invalidate;
pub mod limits;
pub mod outbox;

use std::collections::BTreeMap;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::response::Response;
use axum::routing::any;
use futures_util::{SinkExt, StreamExt};
use rn_api::{SubMessage, SubRequest};
use rn_kernel::feed::{Feed, READ_LIMIT};
use rn_kernel::{Offset, Principal};
use serde_json::Value;

pub use limits::{ConnectionGuard, Connections, Refused};
pub use outbox::Outbox;

use crate::api::query::{self, Named, Scope};
use crate::auth::session::Session;
use crate::state::AppState;

/// How often a quiet socket says it is alive.
pub const HEARTBEAT: Duration = Duration::from_secs(25);

pub fn router() -> Router<AppState> {
    Router::new().route("/api/sub", any(subscribe))
}

async fn subscribe(
    State(state): State<AppState>,
    session: Session,
    upgrade: WebSocketUpgrade,
) -> Response {
    let identity = session
        .principal
        .identity()
        .expect("a Session extractor yields a member");
    match state.connections.admit(identity) {
        Ok(guard) => {
            upgrade.on_upgrade(move |socket| serve(socket, state, session.principal, guard))
        }
        Err(refused) => {
            tracing::info!(?refused, "a subscription was refused at the cap");
            // The socket is accepted and immediately closed rather than
            // refused at HTTP: a browser cannot read a status code off a
            // failed upgrade, but it can read a close code.
            upgrade.on_upgrade(|socket| async move { at_capacity(socket).await })
        }
    }
}

async fn at_capacity(mut socket: WebSocket) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: close_code::AGAIN,
            reason: "subscription limit".into(),
        })))
        .await;
}

/// One connection, from the client's first message to the socket's last.
async fn serve(socket: WebSocket, state: AppState, principal: Principal, guard: ConnectionGuard) {
    let mut connection = Connection {
        state,
        principal,
        queries: Vec::new(),
        cursor: 0,
        outbox: Outbox::default(),
        started: false,
        sets: BTreeMap::new(),
    };
    let (mut sink, mut stream) = socket.split();
    let mut wakes = Box::pin(connection.state.feed.subscribe());
    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;

    loop {
        tokio::select! {
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => connection.hello(&text).await,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            }
            Some(offset) = wakes.next() => connection.woke(offset).await,
            _ = heartbeat.tick() => connection.outbox.push(SubMessage::Heartbeat),
        }
        if !flush(&mut sink, &mut connection.outbox).await {
            break;
        }
    }
    drop(guard);
}

/// Write everything queued. Returns whether the socket is still usable.
async fn flush(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    outbox: &mut Outbox,
) -> bool {
    for message in outbox.drain() {
        let Ok(text) = serde_json::to_string(&message) else {
            continue;
        };
        if sink.send(Message::Text(text.into())).await.is_err() {
            return false;
        }
    }
    true
}

/// One connection's subscription state.
struct Connection {
    state: AppState,
    principal: Principal,
    queries: Vec<Named>,
    cursor: Offset,
    outbox: Outbox,
    started: bool,
    /// What each [`Scope::Set`] query last sent, so a row that leaves the set
    /// can be reported as having left it. Bounded by the queries' own page
    /// sizes, which is what a client is holding anyway.
    sets: BTreeMap<&'static str, BTreeMap<String, Value>>,
}

impl Connection {
    /// Take the client's subscription request. Sending it again replaces the
    /// set, which is how a bundle changes page without reopening the socket.
    async fn hello(&mut self, text: &str) {
        let Ok(request) = serde_json::from_str::<SubRequest>(text) else {
            return;
        };
        if mismatched(text, &self.state.version) {
            // A release changed what a query means; the client's offset no
            // longer describes a set this build would have sent.
            self.outbox.push(SubMessage::Resync);
        }

        self.queries = request
            .subscribe
            .iter()
            .filter_map(|reference| Named::parse(&reference.query))
            .collect();
        self.queries.dedup();
        self.cursor = request.from;
        self.started = true;

        if request.from == 0 {
            self.seed().await;
        } else if request.from > self.head().await {
            // The client is ahead of this node — it read from another node
            // that has since been rewound, or from a release that counted
            // differently. Its position cannot be honoured.
            self.outbox.push(SubMessage::Resync);
            self.cursor = self.head().await;
        }
    }

    /// Send every subscribed query's whole result set, at the current head.
    async fn seed(&mut self) {
        let head = self.head().await;
        for query in self.queries.clone() {
            let rows = match query::read(
                &self.state.store.reads(),
                query,
                &self.principal,
                &query::Params::default(),
            )
            .await
            {
                Ok(rows) => rows,
                Err(error) => {
                    tracing::warn!(%error, query = query.as_str(), "a subscription seed failed");
                    continue;
                }
            };
            if query.scope() == Scope::Set {
                self.sets.insert(query.as_str(), query::remember(&rows));
            }
            let keys: Vec<String> = rows.iter().map(|row| row.key.clone()).collect();
            self.outbox.push(SubMessage::Diff {
                offset: head,
                query: query.as_str().to_owned(),
                ops: query::ops(&keys, rows),
            });
        }
        self.cursor = head;
    }

    /// A wake-up says there is something at or before `offset` to read.
    async fn woke(&mut self, offset: Offset) {
        if !self.started || self.queries.is_empty() || offset <= self.cursor {
            return;
        }
        // A wake-up is a hint, never the event itself: what is delivered comes
        // from the durable read, in order, from our own cursor.
        let events = match self.state.feed.read(self.cursor, READ_LIMIT).await {
            Ok(events) => events,
            Err(error) => {
                tracing::warn!(%error, "the change feed could not be read");
                return;
            }
        };
        for committed in events {
            for query in self.queries.clone() {
                if query.scope() == Scope::Set {
                    self.resend(query, &committed).await;
                    continue;
                }
                let keys = query.changed(&committed.event, &self.principal, self.state.ids());
                if keys.is_empty() {
                    continue;
                }
                let rows = match query::read(
                    &self.state.store.reads(),
                    query,
                    &self.principal,
                    &query::Params::default(),
                )
                .await
                {
                    Ok(rows) => rows,
                    Err(error) => {
                        tracing::warn!(%error, query = query.as_str(), "a diff could not be read");
                        continue;
                    }
                };
                self.outbox.push(SubMessage::Diff {
                    offset: committed.offset,
                    query: query.as_str().to_owned(),
                    ops: query::ops(&keys, rows),
                });
            }
            self.cursor = self.cursor.max(committed.offset);
        }
    }

    /// Re-read a set-scoped query and send what moved.
    ///
    /// The event is a hint about the *kind* of change ([`Named::watches`]);
    /// what is sent is the difference between the set as it is now and the set
    /// this connection last sent, which is the only form that can report a row
    /// leaving a result set the reader shares with other people.
    async fn resend(&mut self, query: Named, committed: &rn_kernel::Committed) {
        if !query.watches(&committed.event) {
            return;
        }
        let rows = match query::read(
            &self.state.store.reads(),
            query,
            &self.principal,
            &query::Params::default(),
        )
        .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, query = query.as_str(), "a diff could not be read");
                return;
            }
        };
        let previous = self.sets.entry(query.as_str()).or_default();
        let ops = query::diff(previous, &rows);
        *previous = query::remember(&rows);
        if ops.is_empty() {
            return;
        }
        self.outbox.push(SubMessage::Diff {
            offset: committed.offset,
            query: query.as_str().to_owned(),
            ops,
        });
    }

    /// Where the feed ends, asked of the feed itself.
    ///
    /// Not a `SELECT` of this module's own: rung 6 swaps the transport behind
    /// `Feed`, and a subscription that carried its own statement would go on
    /// reading a table the new feed no longer owns. A feed that cannot answer
    /// reads as zero, which seeds rather than skips — the safe direction.
    async fn head(&self) -> Offset {
        self.state.feed.head().await.unwrap_or_else(|error| {
            tracing::warn!(%error, "the change feed head could not be read");
            0
        })
    }
}

/// Whether the client stated a release this node is not running.
///
/// The field is read out of the raw message rather than off [`SubRequest`],
/// because the wire type is the contract and a client that predates the field
/// is not mismatched — it is silent, and silence is not a claim. A non-browser
/// client that can set headers gets the same treatment through
/// `X-RN-App-Version`; a browser cannot set headers on a WebSocket, which is
/// why the version rides in the message at all.
#[must_use]
fn mismatched(text: &str, version: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .as_ref()
        .and_then(|json| json.get("version"))
        .and_then(Value::as_str)
        .is_some_and(|stated| stated != version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_that_states_another_release_is_told_to_resync() {
        assert!(mismatched(
            r#"{"subscribe":[],"from":0,"version":"abc123"}"#,
            "dev"
        ));
    }

    #[test]
    fn a_client_that_states_nothing_is_not_mismatched() {
        assert!(!mismatched(r#"{"subscribe":[],"from":0}"#, "dev"));
        assert!(!mismatched(
            r#"{"subscribe":[],"from":0,"version":"dev"}"#,
            "dev"
        ));
        assert!(!mismatched("not json", "dev"));
        assert!(!mismatched(r#"{"version":7}"#, "dev"));
    }

    #[test]
    fn the_heartbeat_is_the_interval_the_client_watchdog_expects() {
        assert_eq!(HEARTBEAT, Duration::from_secs(25));
    }
}
