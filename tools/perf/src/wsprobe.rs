//! The fan-out probe: one command, a hundred sockets, and the distance
//! between the commit and the diff.
//!
//! A hundred subscribers open `WS /api/sub` on `documents` and are seeded at
//! `from: 0`. Once every one of them is quiet, the probe posts a single
//! `edit-document` on the document they all hold a viewer grant on and starts
//! a clock; each socket's latency is the interval from the moment the request
//! left this process to the moment that socket's `Diff` arrived. The command's
//! own reply is timed alongside, so the report can say how much of the
//! delivery was the raft commit and how much was the hub.
//!
//! Two identities' worth of sockets would not do: the per-identity cap is 16,
//! so a hundred sockets is a hundred registrations, which is what the seed
//! makes.

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request;

use crate::http::Conn;
use crate::key;
use crate::stats::Samples;

pub struct Plan {
    pub host: String,
    pub seed: Value,
    /// How many of the seed's subscribers to use.
    pub subscribers: usize,
}

/// What one socket reports back.
enum Note {
    /// Seeded and quiet — ready for the command.
    Ready,
    /// A diff arrived this long after the probe's `go` fired.
    Delivered(Duration),
    /// The socket never got there.
    Lost(String),
}

pub async fn run(plan: Plan) -> Result<Value, String> {
    let subscribers: Vec<String> = plan.seed["subscribers"]
        .as_array()
        .ok_or_else(|| "the seed names no subscribers".to_owned())?
        .iter()
        .filter_map(|entry| entry["token"].as_str().map(str::to_owned))
        .take(plan.subscribers)
        .collect();
    if subscribers.is_empty() {
        return Err("the seed names no subscribers".to_owned());
    }
    let document = plan.seed["fanout"]["document"]
        .as_str()
        .ok_or_else(|| "the seed names no fan-out document".to_owned())?
        .to_owned();
    let owner = plan.seed["owner"]["token"]
        .as_str()
        .ok_or_else(|| "the seed names no owner token".to_owned())?
        .to_owned();

    let (notes, mut inbox) = mpsc::channel::<Note>(subscribers.len() * 4);
    let (go, watch) = tokio::sync::watch::channel(None::<Instant>);
    let mut sockets = Vec::new();
    for token in &subscribers {
        sockets.push(tokio::spawn(listen(
            plan.host.clone(),
            token.clone(),
            notes.clone(),
            watch.clone(),
        )));
    }
    drop(notes);

    // Wait for every socket to be seeded, so the measurement starts from a
    // subscription that is up to date rather than one still catching up.
    let mut ready = 0;
    let mut lost = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    while ready < subscribers.len() && Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(10), inbox.recv()).await {
            Ok(Some(Note::Ready)) => ready += 1,
            Ok(Some(Note::Lost(why))) => lost.push(why),
            Ok(Some(Note::Delivered(_))) => {}
            Ok(None) => break,
            Err(_) => break,
        }
    }
    eprintln!("wsprobe: {ready}/{} sockets seeded", subscribers.len());
    if ready == 0 {
        return Err(format!("no socket was seeded — {lost:?}"));
    }
    // The seed diff and the subscription's own bookkeeping are done; anything
    // still in flight would be counted as the probe's delivery.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The command. The clock starts before the request bytes leave, so the
    // measured delivery contains the commit rather than beginning after it.
    let mut conn = Conn::connect(&plan.host)
        .await
        .map_err(|error| error.to_string())?;
    // The revision is read rather than assumed. A command's precondition is a
    // local read, so an `expected_rev` that is right on the leader can be
    // declined by a follower that has not applied the last entry — and a
    // probe that fired into a decline would report a fan-out of nothing.
    let commit;
    let mut attempts = 0;
    loop {
        attempts += 1;
        let rev = current_rev(&mut conn, &owner, &document)
            .await
            .unwrap_or_else(|| plan.seed["fanout"]["rev"].as_u64().unwrap_or(1));
        let body = json!({
            "key": key::uuid(),
            "document": document,
            "expected_rev": rev,
            "body": format!("the edit the fan-out probe measures, attempt {attempts}"),
        })
        .to_string();
        let fired = Instant::now();
        go.send(Some(fired)).map_err(|error| error.to_string())?;
        let reply = conn
            .post(
                "/api/cmd/edit-document",
                Some(&owner),
                ("application/json", &body),
            )
            .await
            .map_err(|error| error.to_string())?;
        if reply.status == 200 {
            commit = reply.took;
            break;
        }
        if attempts >= 5 {
            return Err(format!(
                "the probe's edit answered {} five times — {}",
                reply.status, reply.body
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let mut delivered = Samples::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    while delivered.len() < ready && Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(5), inbox.recv()).await {
            Ok(Some(Note::Delivered(took))) => delivered.push(took, 200),
            Ok(Some(Note::Lost(why))) => lost.push(why),
            Ok(Some(Note::Ready)) => {}
            Ok(None) => break,
            Err(_) => break,
        }
    }
    for socket in sockets {
        socket.abort();
    }

    let summary = delivered.summary(Duration::from_secs(1));
    println!(
        "{}",
        summary.line(&format!("WS /api/sub fan-out ({ready} subscribers)"))
    );
    println!(
        "  the command's own reply took {:.3} ms; {} socket(s) never delivered",
        commit.as_secs_f64() * 1000.0,
        ready.saturating_sub(delivered.len())
    );
    Ok(json!({
        "subscribers": ready,
        "delivered": delivered.len(),
        "undelivered": ready.saturating_sub(delivered.len()),
        "command_reply_ms": commit.as_secs_f64() * 1000.0,
        "profiles": [summary.json("WS /api/sub fan-out (commit → diff)")],
        "lost": lost,
    }))
}

/// One subscriber: connect, subscribe, report seeded, then time the first diff
/// after the probe fires.
async fn listen(
    host: String,
    token: String,
    notes: mpsc::Sender<Note>,
    mut go: tokio::sync::watch::Receiver<Option<Instant>>,
) {
    let request = match Request::builder()
        .method("GET")
        .uri(format!("ws://{host}/api/sub"))
        .header("host", &host)
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", generate_key())
        .header("cookie", format!("rn_session={token}"))
        .body(())
    {
        Ok(request) => request,
        Err(error) => {
            let _ = notes.send(Note::Lost(error.to_string())).await;
            return;
        }
    };
    let mut socket = match connect_async(request).await {
        Ok((socket, _)) => socket,
        Err(error) => {
            let _ = notes.send(Note::Lost(error.to_string())).await;
            return;
        }
    };

    let subscribe = json!({ "subscribe": [{ "query": "documents" }], "from": 0 }).to_string();
    if let Err(error) = socket.send(Message::Text(subscribe.into())).await {
        let _ = notes.send(Note::Lost(error.to_string())).await;
        return;
    }

    // No select over the `go` channel: a diff can only arrive after the
    // command was posted, and the command is posted after `go` is set, so
    // reading the flag when a diff lands is enough — and it keeps a
    // half-received frame from being dropped by a cancelled future.
    let mut seeded = false;
    loop {
        let Some(Ok(message)) = socket.next().await else {
            return;
        };
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if value["type"] != "diff" {
            continue;
        }
        let fired = *go.borrow_and_update();
        match fired {
            // The probe has fired: this is the diff being measured.
            Some(fired) => {
                let _ = notes.send(Note::Delivered(fired.elapsed())).await;
                return;
            }
            // Still the seed.
            None if !seeded => {
                seeded = true;
                let _ = notes.send(Note::Ready).await;
            }
            None => {}
        }
    }
}

/// What revision the fan-out document is at now, as the node this probe is
/// talking to sees it.
async fn current_rev(conn: &mut Conn, token: &str, document: &str) -> Option<u64> {
    let reply = conn
        .get(&format!("/api/q/document?id={document}"), Some(token))
        .await
        .ok()?;
    reply
        .json()
        .ok()?
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row["draft_rev"].as_u64())
}
