//! The fixture server: enough of the API to render the chrome.
//!
//! `crates/server` does not exist yet, and the shell cannot be looked at
//! without a `whoami`. This serves one from a file, answers queries out of
//! `crates/ui/fixtures/`, echoes commands, and pushes one diff followed by
//! heartbeats — so `trunk serve` in a bundle crate shows the real chrome
//! against real wire shapes. It is a development tool: it authenticates
//! nobody, authorises nothing, and never runs anywhere but a laptop.

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    native::main();
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::time::Duration;

    use axum::Router;
    use axum::extract::Path;
    use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::{any, get, post};
    use serde_json::{Value, json};

    /// The port the bundles' `trunk serve --proxy-backend` points at.
    const PORT: u16 = 3199;

    pub fn main() {
        let runtime = tokio::runtime::Runtime::new().expect("a tokio runtime");
        runtime.block_on(serve());
    }

    async fn serve() {
        let app = Router::new()
            .route("/api/whoami", get(whoami))
            .route("/api/q/{name}", get(query))
            .route("/api/cmd/{name}", post(command))
            .route("/api/sub", any(sub));
        let address = SocketAddr::from(([127, 0, 0, 1], PORT));
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .expect("the fixture port is free");
        println!(
            "fixture api on http://{address} (fixtures in {})",
            dir().display()
        );
        axum::serve(listener, app).await.expect("serves");
    }

    fn dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
    }

    /// Every miss is the uniform decline, exactly as the real API answers.
    fn decline() -> Response {
        (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "decline": "declined" })),
        )
            .into_response()
    }

    fn file(name: &str) -> Option<Value> {
        let path = dir().join(format!("{name}.json"));
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    async fn whoami() -> Response {
        match file("whoami") {
            Some(body) => axum::Json(body).into_response(),
            None => decline(),
        }
    }

    async fn query(Path(name): Path<String>) -> Response {
        // No traversal: a query name is a file name in one directory.
        if name.contains('/') || name.contains('.') {
            return decline();
        }
        match file(&format!("q/{name}")) {
            Some(body) => axum::Json(body).into_response(),
            None => decline(),
        }
    }

    /// Echo the arguments back as the command's result, at a rising offset.
    async fn command(Path(name): Path<String>, body: String) -> Response {
        let mut args: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        if let Some(object) = args.as_object_mut() {
            object.remove("key");
        }
        axum::Json(json!({
            "offset": offset(),
            "result": { "command": name, "args": args },
        }))
        .into_response()
    }

    fn offset() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    async fn sub(upgrade: WebSocketUpgrade) -> Response {
        upgrade.on_upgrade(feed)
    }

    /// One diff, then heartbeats — the shape a bundle's store expects.
    async fn feed(mut socket: WebSocket) {
        let diff = json!({
            "type": "diff",
            "offset": offset(),
            "query": "documents",
            "diff": [{
                "op": "put",
                "key": "r_ZmFrZWRvY3VtZW50aWQwMA",
                "row": { "title": "Kernel report", "updated": "2026-08-26" },
            }],
        });
        if socket
            .send(Message::Text(diff.to_string().into()))
            .await
            .is_err()
        {
            return;
        }
        let beat = json!({ "type": "heartbeat" }).to_string();
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if socket
                .send(Message::Text(beat.clone().into()))
                .await
                .is_err()
            {
                return;
            }
        }
    }
}
