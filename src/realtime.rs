//! Realtime application protocol and the Axum/Hiqlite fan-out behind it.
//!
//! The wire protocol is deliberately an invalidation stream. Durable data
//! stays in SQLite; a reconnecting browser refetches current state instead of
//! asking an in-memory socket hub to replay history it does not own.

use leptos::prelude::RenderHtml;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;
pub const HEARTBEAT_MS: u64 = 25_000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        protocol: u16,
        app_version: String,
        #[serde(default)]
        subscriptions: Vec<String>,
    },
    Subscribe {
        subscriptions: Vec<String>,
    },
    Ping,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello {
        protocol: u16,
        app_version: String,
        authenticated: bool,
        capabilities: Vec<String>,
        heartbeat_ms: u64,
    },
    ReleaseAvailable {
        app_version: String,
    },
    AuthorizationChanged {
        authenticated: bool,
        capabilities: Vec<String>,
        reload_required: bool,
    },
    DataChanged {
        models: Vec<String>,
    },
    ResyncRequired,
    Pong,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClusterEvent {
    pub event_id: String,
    pub origin: String,
    pub kind: ClusterEventKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterEventKind {
    DataChanged { models: Vec<String> },
    AuthorizationChanged { target: AuthorizationTarget },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AuthorizationTarget {
    pub session_hash: Option<String>,
    pub identity_id: Option<i64>,
    pub account_id: Option<i64>,
    pub reload_required: bool,
}

#[cfg(feature = "ssr")]
mod server {
    use std::collections::{HashSet, VecDeque};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::Duration;

    use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode, header},
        response::{IntoResponse, Response},
    };
    use futures_util::{SinkExt, StreamExt};
    use hiqlite::Client;
    use tokio::sync::{broadcast, mpsc};
    use tracing::{debug, info, warn};

    use super::{
        AuthorizationTarget, ClientMessage, ClusterEvent, ClusterEventKind, HEARTBEAT_MS,
        PROTOCOL_VERSION, ServerMessage,
    };
    use crate::auth::{Capability, SessionContext, session, store};

    const MAX_MESSAGE_BYTES: usize = 64 * 1024;
    const OUTGOING_CAPACITY: usize = 64;
    const RECENT_EVENT_CAPACITY: usize = 1_024;
    const REAUTHORIZE_EVERY: Duration = Duration::from_secs(75);

    #[derive(Clone)]
    pub struct RealtimeHub {
        tx: broadcast::Sender<ClusterEvent>,
        shutdown: broadcast::Sender<()>,
        listener_healthy: Arc<AtomicBool>,
        node: Arc<str>,
    }

    impl RealtimeHub {
        #[must_use]
        pub fn new(node: String) -> Self {
            let (tx, _) = broadcast::channel(256);
            let (shutdown, _) = broadcast::channel(1);
            Self {
                tx,
                shutdown,
                listener_healthy: Arc::new(AtomicBool::new(false)),
                node: Arc::from(node),
            }
        }

        pub fn start(&self, db: Client) -> tokio::task::JoinHandle<()> {
            let hub = self.clone();
            tokio::spawn(async move {
                hub.listener_healthy.store(true, Ordering::Release);
                let mut shutdown = hub.subscribe_shutdown();
                let mut recent_ids = HashSet::with_capacity(RECENT_EVENT_CAPACITY);
                let mut recent_order = VecDeque::with_capacity(RECENT_EVENT_CAPACITY);
                loop {
                    let event = tokio::select! {
                        event = db.listen_after_start::<ClusterEvent>() => event,
                        _ = shutdown.recv() => break,
                    };
                    match event {
                        Ok(event) => {
                            if !recent_ids.insert(event.event_id.clone()) {
                                debug!(event_id = %event.event_id, "ignored duplicate realtime event");
                                continue;
                            }
                            recent_order.push_back(event.event_id.clone());
                            if recent_order.len() > RECENT_EVENT_CAPACITY
                                && let Some(expired) = recent_order.pop_front()
                            {
                                recent_ids.remove(&expired);
                            }
                            let _ = hub.tx.send(event);
                        }
                        Err(err) => {
                            hub.listener_healthy.store(false, Ordering::Release);
                            warn!(%err, "realtime Hiqlite listener stopped");
                            break;
                        }
                    }
                }
                hub.listener_healthy.store(false, Ordering::Release);
            })
        }

        #[must_use]
        pub fn listener_healthy(&self) -> bool {
            self.listener_healthy.load(Ordering::Acquire)
        }

        fn subscribe(&self) -> broadcast::Receiver<ClusterEvent> {
            self.tx.subscribe()
        }

        fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
            self.shutdown.subscribe()
        }

        pub fn shutdown(&self) {
            let _ = self.shutdown.send(());
        }

        #[must_use]
        pub fn node(&self) -> &str {
            &self.node
        }
    }

    #[derive(Clone)]
    pub struct RealtimeState {
        pub db: Client,
        pub hub: RealtimeHub,
        pub app_version: Arc<str>,
        pub public_origin: Arc<str>,
    }

    impl RealtimeState {
        #[must_use]
        pub fn new(
            db: Client,
            hub: RealtimeHub,
            app_version: String,
            public_origin: String,
        ) -> Self {
            Self {
                db,
                hub,
                app_version: Arc::from(app_version),
                public_origin: Arc::from(public_origin.trim_end_matches('/')),
            }
        }
    }

    pub async fn publish_data_change(
        db: &Client,
        models: impl IntoIterator<Item = impl Into<String>>,
    ) {
        publish(
            db,
            &origin_node(),
            ClusterEventKind::DataChanged {
                models: models.into_iter().map(Into::into).collect(),
            },
        )
        .await;
    }

    pub async fn publish_authorization_change(db: &Client, target: AuthorizationTarget) {
        publish(
            db,
            &origin_node(),
            ClusterEventKind::AuthorizationChanged { target },
        )
        .await;
    }

    fn origin_node() -> String {
        std::env::var("RN_SITE_NODE").unwrap_or_else(|_| "dev".to_string())
    }

    async fn publish(db: &Client, origin: &str, kind: ClusterEventKind) {
        let event = ClusterEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            origin: origin.to_string(),
            kind,
        };
        if let Err(err) = db.notify(&event).await {
            warn!(%err, event_id = %event.event_id, "realtime invalidation publish failed");
        }
    }

    pub async fn upgrade(
        State(state): State<RealtimeState>,
        headers: HeaderMap,
        ws: WebSocketUpgrade,
    ) -> Response {
        if !same_origin(&headers, &state.public_origin) {
            return StatusCode::FORBIDDEN.into_response();
        }

        let session_hash = headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok())
            .and_then(session::token_from_cookie_header)
            .map(|token| session::token_hash(&token));
        let initial_session = match session_hash.as_deref() {
            Some(hash) => store::resolve_session_hash(&state.db, hash)
                .await
                .ok()
                .flatten(),
            None => None,
        };

        ws.max_message_size(MAX_MESSAGE_BYTES)
            .max_frame_size(MAX_MESSAGE_BYTES)
            .on_upgrade(move |socket| run_socket(socket, state, session_hash, initial_session))
            .into_response()
    }

    fn same_origin(headers: &HeaderMap, public_origin: &str) -> bool {
        let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
            return true;
        };
        origin == public_origin || (is_loopback_origin(public_origin) && is_loopback_origin(origin))
    }

    fn is_loopback_origin(origin: &str) -> bool {
        ["http://127.0.0.1", "http://localhost"]
            .into_iter()
            .any(|base| {
                origin == base
                    || origin
                        .strip_prefix(base)
                        .is_some_and(|rest| rest.starts_with(':'))
            })
    }

    async fn run_socket(
        socket: WebSocket,
        state: RealtimeState,
        session_hash: Option<String>,
        mut resolved: Option<SessionContext>,
    ) {
        let (mut sink, mut stream) = socket.split();
        let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(OUTGOING_CAPACITY);
        let writer = tokio::spawn(async move {
            while let Some(message) = out_rx.recv().await {
                let Ok(json) = serde_json::to_string(&message) else {
                    continue;
                };
                if sink.send(Message::Text(json.into())).await.is_err() {
                    return;
                }
            }
            let _ = sink
                .send(Message::Close(Some(CloseFrame {
                    code: 1012,
                    reason: Utf8Bytes::from_static("service restart"),
                })))
                .await;
        });

        let mut subscriptions = HashSet::<String>::new();
        let mut events = state.hub.subscribe();
        let mut shutdown = state.hub.subscribe_shutdown();
        let mut heartbeat = tokio::time::interval(Duration::from_millis(HEARTBEAT_MS));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut reauthorize = tokio::time::interval(REAUTHORIZE_EVERY);
        reauthorize.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let first = ServerMessage::Hello {
            protocol: PROTOCOL_VERSION,
            app_version: state.app_version.to_string(),
            authenticated: resolved.is_some(),
            capabilities: capabilities(&resolved),
            heartbeat_ms: HEARTBEAT_MS,
        };
        if out_tx.send(first).await.is_err() {
            return;
        }

        loop {
            tokio::select! {
                incoming = stream.next() => {
                    let Some(Ok(message)) = incoming else { break };
                    match message {
                        Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Hello { protocol, app_version, subscriptions: requested }) => {
                                if protocol != PROTOCOL_VERSION { break; }
                                subscriptions = requested.into_iter().collect();
                                if app_version != state.app_version.as_ref()
                                    && send_now(&out_tx, ServerMessage::ReleaseAvailable { app_version: state.app_version.to_string() }).is_err()
                                { break; }
                            }
                            Ok(ClientMessage::Subscribe { subscriptions: requested }) => {
                                subscriptions = requested.into_iter().collect();
                            }
                            Ok(ClientMessage::Ping) => {
                                if send_now(&out_tx, ServerMessage::Pong).is_err() { break; }
                            }
                            Err(err) => debug!(%err, "ignored invalid realtime client message"),
                        },
                        Message::Ping(bytes) => {
                            // The browser API cannot emit protocol ping frames, but native
                            // clients can. A text Pong is enough for the application clock.
                            drop(bytes);
                            if send_now(&out_tx, ServerMessage::Pong).is_err() { break; }
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
                event = events.recv() => match event {
                    Ok(event) => match event.kind {
                        ClusterEventKind::DataChanged { models }
                            if subscriptions.contains("admin")
                                && resolved.as_ref().is_some_and(|s| s.capabilities.has(Capability::Manage)) => {
                            if send_now(&out_tx, ServerMessage::DataChanged { models }).is_err() { break; }
                        }
                        ClusterEventKind::AuthorizationChanged { target }
                            if target_matches(&target, &resolved, session_hash.as_deref()) => {
                            let next = resolve_hash(&state.db, session_hash.as_deref()).await;
                            resolved = next;
                            if send_now(&out_tx, ServerMessage::AuthorizationChanged {
                                authenticated: resolved.is_some(),
                                capabilities: capabilities(&resolved),
                                reload_required: target.reload_required,
                            }).is_err() { break; }
                        }
                        _ => {}
                    },
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "realtime connection lagged; requesting resync");
                        if send_now(&out_tx, ServerMessage::ResyncRequired).is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = reauthorize.tick(), if session_hash.is_some() => {
                    let next = resolve_hash(&state.db, session_hash.as_deref()).await;
                    if authorization_shape(&next) != authorization_shape(&resolved) {
                        resolved = next;
                        if send_now(&out_tx, ServerMessage::AuthorizationChanged {
                            authenticated: resolved.is_some(),
                            capabilities: capabilities(&resolved),
                            reload_required: true,
                        }).is_err() { break; }
                    }
                }
                _ = heartbeat.tick() => {
                    if send_now(&out_tx, ServerMessage::Pong).is_err() { break; }
                }
                _ = shutdown.recv() => break,
            }
        }

        info!(node = state.hub.node(), "realtime connection closed");
        drop(out_tx);
        let _ = writer.await;
    }

    fn send_now(tx: &mpsc::Sender<ServerMessage>, message: ServerMessage) -> Result<(), ()> {
        tx.try_send(message).map_err(|_| ())
    }

    async fn resolve_hash(db: &Client, hash: Option<&str>) -> Option<SessionContext> {
        match hash {
            Some(hash) => store::resolve_session_hash(db, hash).await.ok().flatten(),
            None => None,
        }
    }

    fn capabilities(session: &Option<SessionContext>) -> Vec<String> {
        session
            .as_ref()
            .map(|session| {
                session
                    .capabilities
                    .iter()
                    .map(|capability| capability.as_str().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn authorization_shape(session: &Option<SessionContext>) -> (bool, Vec<String>) {
        (session.is_some(), capabilities(session))
    }

    fn target_matches(
        target: &AuthorizationTarget,
        session: &Option<SessionContext>,
        session_hash: Option<&str>,
    ) -> bool {
        if target
            .session_hash
            .as_deref()
            .is_some_and(|wanted| Some(wanted) != session_hash)
        {
            return false;
        }
        let Some(session) = session else {
            return target.session_hash.as_deref() == session_hash;
        };
        target
            .identity_id
            .is_none_or(|wanted| wanted == session.identity_id.get())
            && target
                .account_id
                .is_none_or(|wanted| wanted == session.account_id.get())
    }
}

#[cfg(feature = "ssr")]
pub use server::{
    RealtimeHub, RealtimeState, publish_authorization_change, publish_data_change, upgrade,
};

#[cfg(feature = "hydrate")]
mod client {
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;

    use futures_util::{SinkExt, StreamExt};
    use gloo_net::{
        http::Request,
        websocket::{Message as WsMessage, futures::WebSocket},
    };
    use leptos::prelude::*;
    use wasm_bindgen::{JsCast, JsValue, closure::Closure};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{CustomEvent, CustomEventInit, window};

    use super::{ClientMessage, PROTOCOL_VERSION, ServerMessage};
    use crate::app::APP_VERSION;

    type FlushFuture = Pin<Box<dyn Future<Output = bool>>>;

    struct Guard {
        is_dirty: Rc<dyn Fn() -> bool>,
        flush: Rc<dyn Fn() -> FlushFuture>,
    }

    thread_local! {
        static GUARDS: RefCell<BTreeMap<u32, Guard>> = const { RefCell::new(BTreeMap::new()) };
        static NEXT_GUARD: Cell<u32> = const { Cell::new(1) };
    }

    /// Removes a reload guard when its editor unmounts.
    pub struct ReloadGuardHandle(u32);

    impl Drop for ReloadGuardHandle {
        fn drop(&mut self) {
            GUARDS.with_borrow_mut(|guards| {
                guards.remove(&self.0);
            });
        }
    }

    /// Register an editor with the global release reload coordinator.
    pub fn register_reload_guard(
        is_dirty: impl Fn() -> bool + 'static,
        flush: impl Fn() -> FlushFuture + 'static,
    ) -> ReloadGuardHandle {
        let id = NEXT_GUARD.with(|next| {
            let id = next.get();
            next.set(id.wrapping_add(1).max(1));
            id
        });
        GUARDS.with_borrow_mut(|guards| {
            guards.insert(
                id,
                Guard {
                    is_dirty: Rc::new(is_dirty),
                    flush: Rc::new(flush),
                },
            );
        });
        ReloadGuardHandle(id)
    }

    pub fn start(update_available: RwSignal<bool>) {
        restore_view_state();
        install_visibility_version_check(update_available);
        spawn_local(async move {
            let mut attempt = 0_u32;
            let mut connected_once = false;
            loop {
                let Some(url) = websocket_url() else { return };
                match WebSocket::open(&url) {
                    Ok(socket) => {
                        let (mut write, mut read) = socket.split();
                        let hello = ClientMessage::Hello {
                            protocol: PROTOCOL_VERSION,
                            app_version: APP_VERSION.to_string(),
                            subscriptions: vec!["system".into(), "admin".into()],
                        };
                        if let Ok(json) = serde_json::to_string(&hello) {
                            let _ = write.send(WsMessage::Text(json)).await;
                        }
                        attempt = 0;
                        while let Some(Ok(message)) = read.next().await {
                            let WsMessage::Text(text) = message else {
                                continue;
                            };
                            let Ok(message) = serde_json::from_str::<ServerMessage>(&text) else {
                                continue;
                            };
                            if matches!(message, ServerMessage::Hello { .. }) {
                                if connected_once {
                                    // The invalidation stream is intentionally not
                                    // durable. A fresh authoritative read closes the
                                    // gap left by any events missed while disconnected.
                                    dispatch("rn-resync-required", &[]);
                                }
                                connected_once = true;
                            }
                            handle(message, update_available);
                        }
                    }
                    Err(error) => web_sys::console::warn_2(
                        &"realtime connection failed".into(),
                        &JsValue::from_str(&error.to_string()),
                    ),
                }
                check_version(update_available).await;
                attempt = attempt.saturating_add(1).min(6);
                let ceiling = (1_u32 << attempt).min(60) as f64 * 1000.0;
                delay(js_sys::Math::random() * ceiling).await;
            }
        });
    }

    fn handle(message: ServerMessage, update_available: RwSignal<bool>) {
        match message {
            ServerMessage::Hello { app_version, .. }
            | ServerMessage::ReleaseAvailable { app_version }
                if app_version != APP_VERSION =>
            {
                schedule_release(update_available)
            }
            ServerMessage::AuthorizationChanged {
                authenticated,
                capabilities,
                reload_required,
            } => {
                let path = window()
                    .and_then(|window| window.location().pathname().ok())
                    .unwrap_or_default();
                let forbidden = (!authenticated
                    && (path == "/protected" || path.starts_with("/manage")))
                    || (path.starts_with("/manage")
                        && !capabilities.iter().any(|capability| capability == "manage"));
                if forbidden || reload_required {
                    clear_view_state();
                    reload_now();
                } else {
                    dispatch("rn-authorization-changed", &capabilities);
                }
            }
            ServerMessage::DataChanged { models } => dispatch("rn-data-changed", &models),
            ServerMessage::ResyncRequired => dispatch("rn-resync-required", &[]),
            _ => {}
        }
    }

    fn schedule_release(update_available: RwSignal<bool>) {
        if update_available.get_untracked() {
            return;
        }
        update_available.set(true);
        spawn_local(flush_and_reload(true));
    }

    pub fn retry_release() {
        spawn_local(flush_and_reload(false));
    }

    async fn flush_and_reload(jitter: bool) {
        let flushes = GUARDS.with_borrow(|guards| {
            guards
                .values()
                .filter(|guard| (guard.is_dirty)())
                .map(|guard| Rc::clone(&guard.flush))
                .collect::<Vec<_>>()
        });
        for flush in flushes {
            if !(flush)().await {
                return;
            }
        }
        capture_view_state();
        if jitter {
            delay(js_sys::Math::random() * 30_000.0).await;
        }
        reload_now();
    }

    fn websocket_url() -> Option<String> {
        let window = window()?;
        let location = window.location();
        let scheme = if location.protocol().ok()?.as_str() == "https:" {
            "wss"
        } else {
            "ws"
        };
        Some(format!("{scheme}://{}/api/realtime", location.host().ok()?))
    }

    async fn delay(ms: f64) {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            let callback = Closure::<dyn FnMut()>::once(move || {
                let _ = resolve.call0(&JsValue::UNDEFINED);
            });
            if let Some(window) = window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    ms as i32,
                );
            }
            callback.forget();
        });
        let _ = JsFuture::from(promise).await;
    }

    async fn check_version(update_available: RwSignal<bool>) {
        let Ok(response) = Request::get("/version").send().await else {
            return;
        };
        let Ok(version) = response.text().await else {
            return;
        };
        if version.trim() != APP_VERSION {
            schedule_release(update_available);
        }
    }

    fn install_visibility_version_check(update_available: RwSignal<bool>) {
        let callback = Closure::<dyn FnMut()>::new(move || {
            if window()
                .and_then(|window| window.document())
                .is_some_and(|document| {
                    document.visibility_state() == web_sys::VisibilityState::Visible
                })
            {
                spawn_local(check_version(update_available));
            }
        });
        if let Some(document) = window().and_then(|window| window.document()) {
            let _ = document.add_event_listener_with_callback(
                "visibilitychange",
                callback.as_ref().unchecked_ref(),
            );
        }
        callback.forget();
    }

    fn dispatch(name: &str, values: &[String]) {
        let array = js_sys::Array::new();
        for value in values {
            array.push(&JsValue::from_str(value));
        }
        let init = CustomEventInit::new();
        init.set_detail(&array);
        if let (Some(window), Ok(event)) =
            (window(), CustomEvent::new_with_event_init_dict(name, &init))
        {
            let _ = window.dispatch_event(&event);
        }
    }

    const VIEW_STATE_KEY: &str = "rn.reload-view-state.v1";

    fn capture_view_state() {
        let Some(browser_window) = window() else {
            return;
        };
        let Ok(Some(storage)) = browser_window.session_storage() else {
            return;
        };
        let modals = browser_window
            .document()
            .and_then(|document| {
                document
                    .query_selector_all("[data-reload-modal][open]")
                    .ok()
            })
            .map(|nodes| {
                (0..nodes.length())
                    .filter_map(|index| nodes.item(index))
                    .filter_map(|node| node.dyn_into::<web_sys::Element>().ok())
                    .filter_map(|element| element.get_attribute("data-reload-modal"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let state = serde_json::json!({
            "path": browser_window.location().pathname().unwrap_or_default(),
            "x": browser_window.scroll_x().unwrap_or(0.0),
            "y": browser_window.scroll_y().unwrap_or(0.0),
            "modals": modals,
        });
        let _ = storage.set_item(VIEW_STATE_KEY, &state.to_string());
    }

    fn restore_view_state() {
        let Some(browser_window) = window() else {
            return;
        };
        let Ok(Some(storage)) = browser_window.session_storage() else {
            return;
        };
        let Ok(Some(raw)) = storage.get_item(VIEW_STATE_KEY) else {
            return;
        };
        let _ = storage.remove_item(VIEW_STATE_KEY);
        let Ok(state) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return;
        };
        if state["path"].as_str() == browser_window.location().pathname().ok().as_deref() {
            let x = state["x"].as_f64().unwrap_or(0.0);
            let y = state["y"].as_f64().unwrap_or(0.0);
            let callback = Closure::<dyn FnMut()>::once(move || {
                if let Some(window) = window() {
                    window.scroll_to_with_x_and_y(x, y);
                }
            });
            let _ = browser_window.request_animation_frame(callback.as_ref().unchecked_ref());
            callback.forget();

            let open = state["modals"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|value| value.as_str())
                .collect::<std::collections::HashSet<_>>();
            if let Some(document) = browser_window.document() {
                if let Ok(nodes) = document.query_selector_all("[data-reload-modal]") {
                    for index in 0..nodes.length() {
                        let Some(node) = nodes.item(index) else {
                            continue;
                        };
                        let Ok(element) = node.dyn_into::<web_sys::Element>() else {
                            continue;
                        };
                        if element
                            .get_attribute("data-reload-modal")
                            .as_deref()
                            .is_some_and(|id| open.contains(id))
                        {
                            let _ = element.set_attribute("open", "");
                        }
                    }
                }
            }
        }
    }

    fn clear_view_state() {
        if let Some(window) = window() {
            if let Ok(Some(storage)) = window.session_storage() {
                let _ = storage.remove_item(VIEW_STATE_KEY);
            }
        }
    }

    fn reload_now() {
        if let Some(window) = window() {
            let _ = window.location().reload();
        }
    }
}

#[cfg(feature = "hydrate")]
pub use client::{ReloadGuardHandle, register_reload_guard};

#[leptos::prelude::island]
pub fn RealtimeWatcher() -> impl leptos::prelude::IntoView {
    use leptos::prelude::*;

    let update_available = RwSignal::new(false);
    #[cfg(feature = "hydrate")]
    crate::realtime::client::start(update_available);

    view! {
        <Show when=move || update_available.get()>
            <aside
                role="status"
                style="position:fixed;right:1rem;bottom:1rem;z-index:10000;padding:.75rem 1rem;border:1px solid var(--border);background:var(--bg-muted);color:var(--fg);box-shadow:0 .5rem 2rem #0008"
            >
                <span>"A newer version is available. "</span>
                <button
                    type="button"
                    on:click=move |_| {
                        #[cfg(feature = "hydrate")]
                        crate::realtime::client::retry_release();
                    }
                >
                    "Reload now"
                </button>
            </aside>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_round_trips_tagged_messages() {
        let message = ServerMessage::DataChanged {
            models: vec!["sessions".into()],
        };
        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"type\":\"data_changed\""));
        assert_eq!(
            serde_json::from_str::<ServerMessage>(&json).unwrap(),
            message
        );

        let authorization = ClusterEvent {
            event_id: "event-1".into(),
            origin: "node-2".into(),
            kind: ClusterEventKind::AuthorizationChanged {
                target: AuthorizationTarget {
                    identity_id: Some(42),
                    reload_required: true,
                    ..Default::default()
                },
            },
        };
        let json = serde_json::to_string(&authorization).unwrap();
        let decoded = serde_json::from_str::<ClusterEvent>(&json).unwrap();
        match decoded.kind {
            ClusterEventKind::AuthorizationChanged { target } => {
                assert_eq!(target.identity_id, Some(42));
                assert!(target.reload_required);
            }
            ClusterEventKind::DataChanged { .. } => panic!("wrong event kind"),
        }

        let encoded = bincode::serde::encode_to_vec(&authorization, bincode::config::legacy())
            .expect("Hiqlite's bincode wire format must accept cluster events");
        let _: (ClusterEvent, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::legacy())
                .expect("cluster events must round-trip through Hiqlite");
    }
}
