//! Runtime configuration, sourced from the environment.

/// Process configuration resolved once at startup.
///
/// Add new tunables here (and read them in [`Config::from_env`]) rather than
/// reaching for `std::env::var` from inside handlers.
pub struct Config {
    /// Address the HTTP server binds to.
    pub bind_addr: String,
}

impl Config {
    /// Loads configuration from the environment, falling back to local defaults.
    pub fn from_env() -> Self {
        let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
        Self { bind_addr }
    }
}
