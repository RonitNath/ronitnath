//! `WS /api/sub` against a real node: a command in one client, a diff in
//! another.
//!
//! The socket needs an HTTP transport rather than the in-memory one, so this
//! binary builds its server differently from the rest of the suite — an
//! upgrade is a real upgrade or it is not a test of one.

mod harness;

use axum_test::{TestServer, TestServerConfig, TestWebSocket, Transport, WsMessage};
use rn_site::sub::limits::PER_IDENTITY;
use serde_json::{Value, json};
use tokio::sync::OnceCell;

static NODE: OnceCell<rn_site::AppState> = OnceCell::const_new();

async fn state() -> rn_site::AppState {
    harness::node(&NODE, "127.0.0.1:8198", "127.0.0.1:8298").await
}

fn server(state: &rn_site::AppState) -> TestServer {
    TestServerConfig {
        transport: Some(Transport::HttpRandomPort),
        ..TestServerConfig::new()
    }
    .build(rn_site::router(state.clone()))
}

/// Open a subscription as somebody, and say what it wants.
async fn subscribe(server: &TestServer, token: &str, queries: &[&str], from: u64) -> TestWebSocket {
    let mut socket = server
        .get_websocket("/api/sub")
        .add_header("cookie", format!("rn_session={token}"))
        .await
        .into_websocket()
        .await;
    socket
        .send_json(&json!({
            "subscribe": queries.iter().map(|query| json!({"query": query})).collect::<Vec<_>>(),
            "from": from,
        }))
        .await;
    socket
}

/// Add an email factor to somebody's identity — one command, one event, one
/// key moved in the `identities` query.
async fn touch(server: &TestServer, token: &str, tag: &str) -> Value {
    server
        .post("/api/cmd/add-factor")
        .add_header("cookie", format!("rn_session={token}"))
        .add_header("sec-fetch-site", "same-origin")
        .text(format!(
            r#"{{"key":"{}","kind":"email","value":"sub-{tag}@example.invalid"}}"#,
            uuid_from(tag)
        ))
        .content_type("application/json")
        .await
        .json()
}

/// A stable idempotency key per tag, so a retried nudge is a replay rather
/// than a second command.
fn uuid_from(tag: &str) -> String {
    let mut hex = String::new();
    for byte in tag.bytes().chain(std::iter::repeat(b'0')).take(16) {
        hex.push_str(&format!("{byte:02x}"));
    }
    format!(
        "{}-{}-4{}-8{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}

/// The next message that is not a heartbeat.
///
/// A wake-up is a hint and may be lost (`kernel::feed`), so a silence is
/// answered by nudging the feed rather than by waiting out the 25-second
/// heartbeat — the test is about what the socket *sends*, not about how
/// reliable hiqlite's listen channel is under a burst.
async fn next_message<F, Fut>(socket: &mut TestWebSocket, mut nudge: F) -> Value
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for attempt in 0..12 {
        let received = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            socket.receive_json::<Value>(),
        )
        .await;
        match received {
            Ok(message) if message["type"] != "heartbeat" => return message,
            Ok(_) => {}
            Err(_) => nudge(attempt).await,
        }
    }
    panic!("nothing but silence and heartbeats arrived on the socket");
}

/// For a socket that is seeded on connect and needs no nudge.
async fn next_diff(socket: &mut TestWebSocket) -> Value {
    next_message(socket, |_| async {}).await
}

#[test]
fn a_fresh_socket_is_seeded_with_the_whole_result_set() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let somebody = harness::register(&state, "Seeded", "sub-seed@example.invalid").await;

        let mut socket = subscribe(&server, &somebody.token, &["sessions"], 0).await;
        let diff = next_diff(&mut socket).await;

        assert_eq!(diff["type"], "diff");
        assert_eq!(diff["query"], "sessions");
        let ops = diff["diff"].as_array().expect("ops");
        assert_eq!(ops.len(), 1, "one device, one row");
        assert_eq!(ops[0]["op"], "put");
        assert!(
            ops[0]["row"]["public_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("s_"))
        );
        // The seed carries the offset it is current as of, so the client's
        // next resume asks for what happened after it and not for all of it.
        assert!(diff["offset"].as_u64().is_some_and(|offset| offset > 0));

        socket.close().await;
    });
}

#[test]
fn a_command_in_one_client_arrives_as_a_diff_in_another_at_its_own_offset() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let somebody = harness::register(&state, "Watcher", "sub-watch@example.invalid").await;

        let mut socket = subscribe(&server, &somebody.token, &["sessions"], 0).await;
        let seed = next_diff(&mut socket).await;
        let seeded_at = seed["offset"].as_u64().expect("a seed offset");

        // A command from a second client: a new factor on the same identity,
        // and therefore a key that moved in the identities result set.
        let reply = touch(&server, &somebody.token, "watch-second").await;
        let offset = reply["offset"].as_u64().expect("the command's offset");
        assert!(offset > seeded_at);

        // Resubscribe to the query that moved, from the offset just seen, and
        // watch the next command land.
        let mut watcher = subscribe(&server, &somebody.token, &["identities"], offset).await;
        let second = touch(&server, &somebody.token, "watch-third").await;
        let second_offset = second["offset"].as_u64().expect("an offset");

        let diff = next_message(&mut watcher, |_| {
            let server = &server;
            let token = somebody.token.clone();
            async move {
                touch(server, &token, "watch-third").await;
            }
        })
        .await;
        assert_eq!(diff["type"], "diff");
        assert_eq!(diff["query"], "identities");
        assert_eq!(
            diff["offset"].as_u64(),
            Some(second_offset),
            "the diff must carry the offset of the command that caused it"
        );
        let ops = diff["diff"].as_array().expect("ops");
        assert_eq!(ops[0]["op"], "put");
        let factors = ops[0]["row"]["factors"].as_array().expect("factors");
        assert_eq!(factors.len(), 4, "email, password, and the two just added");

        watcher.close().await;
        socket.close().await;
    });
}

#[test]
fn a_client_running_another_release_is_told_to_resync() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let somebody = harness::register(&state, "Stale", "sub-release@example.invalid").await;

        let mut socket = server
            .get_websocket("/api/sub")
            .add_header("cookie", format!("rn_session={}", somebody.token))
            .await
            .into_websocket()
            .await;
        socket
            .send_json(&json!({
                "subscribe": [{"query": "sessions"}],
                "from": 0,
                "version": "a-release-this-node-is-not-running",
            }))
            .await;

        let message: Value = socket.receive_json().await;
        assert_eq!(
            message["type"], "resync",
            "a release mismatch must precede any diff"
        );
        socket.close().await;
    });
}

#[test]
fn a_client_ahead_of_this_node_is_told_to_resync() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let somebody = harness::register(&state, "Ahead", "sub-ahead@example.invalid").await;

        // An offset no node has reached: the client read from somewhere that
        // has since been rewound, and its position cannot be honoured.
        let mut socket = subscribe(&server, &somebody.token, &["sessions"], 1_000_000).await;
        let message: Value = socket.receive_json().await;
        assert_eq!(message["type"], "resync");
        socket.close().await;
    });
}

#[test]
fn falling_behind_the_bound_drops_the_backlog_and_resyncs() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let somebody = harness::register(&state, "Behind", "sub-behind@example.invalid").await;
        let cookie = format!("rn_session={}", somebody.token);

        // Run more commands than one connection may hold messages for, with
        // nobody subscribed. Then subscribe from the beginning of that run and
        // wake the socket once: the whole backlog arrives in a single pass,
        // overflows the queue, and is replaced by one resync.
        let mut first = 0;
        let backlog = rn_site::sub::outbox::CAPACITY + 8;
        for step in 0..backlog {
            let reply: Value = server
                .post("/api/cmd/add-factor")
                .add_header("cookie", cookie.clone())
                .add_header("sec-fetch-site", "same-origin")
                .text(format!(
                    r#"{{"key":"2c3d4e5f-0000-4000-8000-{step:012}",
                         "kind":"email","value":"sub-behind-{step}@example.invalid"}}"#
                ))
                .content_type("application/json")
                .await
                .json();
            if step == 0 {
                first = reply["offset"].as_u64().expect("an offset");
            }
        }

        let mut socket = subscribe(&server, &somebody.token, &["identities"], first).await;
        // One more command, so the socket wakes and reads the whole backlog in
        // a single pass.
        let message = next_message(&mut socket, |_| {
            let server = &server;
            let token = somebody.token.clone();
            async move {
                touch(server, &token, "behind-last").await;
            }
        })
        .await;
        assert_eq!(
            message["type"], "resync",
            "a backlog over the bound must be dropped whole, not sent"
        );
        socket.close().await;
    });
}

#[test]
fn the_seventeenth_socket_is_closed_rather_than_served() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        let somebody = harness::register(&state, "Many", "sub-cap@example.invalid").await;

        let mut held = Vec::new();
        for _ in 0..PER_IDENTITY {
            held.push(subscribe(&server, &somebody.token, &["sessions"], 0).await);
        }

        let mut over = server
            .get_websocket("/api/sub")
            .add_header("cookie", format!("rn_session={}", somebody.token))
            .await
            .into_websocket()
            .await;
        match over.receive_message().await {
            WsMessage::Close(Some(frame)) => assert_eq!(
                u16::from(frame.code),
                1013,
                "the cap must say try again later, not never"
            ),
            other => panic!("the socket over the cap was served: {other:?}"),
        }

        for socket in held {
            socket.close().await;
        }
    });
}

#[test]
fn a_member_who_subscribes_to_the_deployments_lists_is_told_nothing_about_them() {
    harness::run(async {
        let state = state().await;
        let server = server(&state);
        // Two ordinary members. Neither holds `platform:* #operator`, and the
        // socket does not check tiers — a name is a name.
        let nosy = harness::register(&state, "Nosy", "sub-nosy@example.invalid").await;
        let other = harness::register(&state, "Other", "sub-other@example.invalid").await;

        // Resume from where the feed already is, so nothing here is a seed:
        // what arrives afterwards arrives because an event named a key.
        let here = touch(&server, &nosy.token, "leak-start").await["offset"]
            .as_u64()
            .expect("an offset");
        let mut socket = subscribe(
            &server,
            &nosy.token,
            &["platform-parties", "platform-identities", "platform-audit"],
            here,
        )
        .await;

        // Somebody else's command, on rows this reader has no claim to. The
        // platform lists name their keys off the event and are guarded only
        // on the read, so a decline that reads as an empty result set turns
        // every one of those keys into a `del` addressed to this socket.
        for tag in ["leak-a", "leak-b", "leak-c"] {
            touch(&server, &other.token, tag).await;
        }

        for _ in 0..8 {
            match tokio::time::timeout(
                std::time::Duration::from_millis(400),
                socket.receive_json::<Value>(),
            )
            .await
            {
                Ok(message) if message["type"] == "heartbeat" => {}
                Ok(message) => panic!(
                    "a member who cannot read the deployment's lists was sent {}",
                    message
                ),
                Err(_) => {}
            }
        }

        socket.close().await;
    });
}
