//! Fixed-window per-client rate limiting for unauthenticated write routes.
//!
//! Not built for scale-out — the counters live in process memory, so a
//! multi-instance deployment gets one budget per instance. Swap for a
//! shared-store limiter (e.g. Redis-backed) if that ever matters; until
//! then this is enough to blunt a single abusive client hammering
//! `/api/client-errors`.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

const WINDOW: Duration = Duration::from_secs(60);

struct Bucket {
    window_start: Instant,
    count: u32,
}

/// Shared limiter state. Cheap to clone (an `Arc` inside); register the same
/// instance's [`enforce`] middleware on every write route you want sharing
/// one budget (see `app.rs` for the client-error route).
#[derive(Clone)]
pub struct RateLimiter {
    max_per_window: u32,
    trust_proxy: bool,
    buckets: Arc<Mutex<HashMap<IpAddr, Bucket>>>,
}

impl RateLimiter {
    pub fn new(max_per_window: u32, trust_proxy: bool) -> Self {
        Self {
            max_per_window,
            trust_proxy,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Records one hit for `ip`; returns whether it's still within budget.
    fn allow(&self, ip: IpAddr) -> bool {
        let mut buckets = self.buckets.lock().expect("rate limiter mutex poisoned");
        let now = Instant::now();
        let bucket = buckets.entry(ip).or_insert_with(|| Bucket {
            window_start: now,
            count: 0,
        });
        if now.duration_since(bucket.window_start) >= WINDOW {
            bucket.window_start = now;
            bucket.count = 0;
        }
        bucket.count += 1;
        bucket.count <= self.max_per_window
    }

    /// The client IP to key on: the forwarded header when `trust_proxy` is
    /// set (only trustworthy behind ingress that sets it itself — see
    /// `Config::trust_proxy`), otherwise the raw TCP peer address.
    fn client_ip(&self, addr: SocketAddr, request: &Request) -> IpAddr {
        if self.trust_proxy {
            let forwarded = request
                .headers()
                .get(header::HeaderName::from_static("x-forwarded-for"))
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(',').next())
                .and_then(|first| first.trim().parse::<IpAddr>().ok());
            if let Some(ip) = forwarded {
                return ip;
            }
        }
        addr.ip()
    }
}

#[derive(Serialize)]
struct LimitedBody {
    error: &'static str,
}

/// Applies `limiter` to mutating requests only (`GET`/`HEAD` always pass) —
/// register via `axum::middleware::from_fn_with_state(limiter, enforce)` on
/// just the routes that need it (see `app.rs`), not the whole router.
pub async fn enforce(
    State(limiter): State<RateLimiter>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    if matches!(*request.method(), Method::GET | Method::HEAD) {
        return next.run(request).await;
    }

    let ip = limiter.client_ip(addr, &request);
    if !limiter.allow(ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(LimitedBody {
                error: "rate limit exceeded, try again later",
            }),
        )
            .into_response();
    }

    next.run(request).await
}
