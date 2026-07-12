//! Runtime configuration, sourced from the environment.

use std::str::FromStr;

/// Process configuration resolved once at startup.
///
/// Add new tunables here (and read them in [`Config::from_env`]) rather than
/// reaching for `std::env::var` from inside handlers.
pub struct Config {
    /// Address the public site server binds to.
    pub bind_addr: String,
    /// Address the authenticated admin server binds to.
    pub admin_bind_addr: String,
    /// sqlite connection string. Defaults to a repo-relative file so a fresh
    /// fork runs with zero setup; override via `.env` when you need sqlx-cli
    /// (see AGENTS.md).
    pub database_url: String,
    /// Per-request timeout; a hung handler is aborted and answered with a
    /// bare 408 rather than pinning the connection forever.
    pub request_timeout_secs: u64,
    /// Maximum accepted request body size, in bytes. Applies to every
    /// route — raise it per-handler later if a specific upload needs more.
    pub max_body_bytes: usize,
    /// Requests per minute allowed per client on unauthenticated write
    /// endpoints (see [`crate::rate_limit`]).
    pub rate_limit_per_minute: u32,
    /// Whether to trust `X-Forwarded-For` for the client IP used by rate
    /// limiting, instead of the raw TCP peer address. Only set this when
    /// the server sits behind ingress that sets the header itself —
    /// otherwise a client can spoof it to dodge the limiter.
    pub trust_proxy: bool,
    /// Sets the session cookie's `Secure` flag and `__Host-` prefix. Only
    /// true once the server is actually behind TLS — a plain HTTP local
    /// dev server can't set a `Secure` cookie the browser will accept.
    pub cookie_secure: bool,
    /// Session lifetime (sliding — see `sessions.last_seen_at`).
    pub session_ttl_secs: i64,
    /// `open` (default) lets anyone hit `/signup`; `closed` returns 404
    /// there for deployments that provision identities out-of-band.
    pub signup_open: bool,
    /// JSON provider registry for OIDC relying-party login. Missing file =
    /// no configured providers and no login-page changes.
    pub oidc_providers_path: String,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_or_parse<T: FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

impl Config {
    /// Loads configuration from the environment, falling back to local defaults.
    pub fn from_env() -> Self {
        Self {
            bind_addr: env_or("BIND_ADDR", "127.0.0.1:3130"),
            admin_bind_addr: env_or("ADMIN_BIND_ADDR", "127.0.0.1:3131"),
            database_url: env_or("DATABASE_URL", "sqlite:data/app.db"),
            request_timeout_secs: env_or_parse("REQUEST_TIMEOUT_SECS", 30),
            max_body_bytes: env_or_parse("MAX_BODY_BYTES", 1_048_576),
            rate_limit_per_minute: env_or_parse("RATE_LIMIT_PER_MINUTE", 10),
            trust_proxy: env_or_parse("TRUSTED_PROXY", false),
            cookie_secure: env_or_parse("COOKIE_SECURE", false),
            session_ttl_secs: env_or_parse("SESSION_TTL_SECS", 30 * 24 * 60 * 60),
            signup_open: env_or("AUTH_SIGNUP", "open") != "closed",
            oidc_providers_path: env_or("OIDC_PROVIDERS_PATH", "data/oidc_providers.json"),
        }
    }

    /// Config for router tests: in-memory-sized limits so tests can exercise
    /// the body-limit and rate-limit layers without slow/huge payloads.
    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".into(),
            admin_bind_addr: "127.0.0.1:0".into(),
            database_url: "sqlite::memory:".into(),
            request_timeout_secs: 30,
            max_body_bytes: 1024,
            rate_limit_per_minute: 10,
            trust_proxy: false,
            cookie_secure: false,
            session_ttl_secs: 30 * 24 * 60 * 60,
            signup_open: true,
            oidc_providers_path: "data/missing-test-oidc-providers.json".into(),
        }
    }
}
