//! Runtime configuration, sourced from the environment.

/// Process configuration resolved once at startup.
///
/// Add new tunables here (and read them in [`Config::from_env`]) rather than
/// reaching for `std::env::var` from inside handlers.
pub struct Config {
    /// Address the HTTP server binds to.
    pub bind_addr: String,
    /// sqlite connection string. Defaults to a repo-relative file so a fresh
    /// fork runs with zero setup; override via `.env` when you need sqlx-cli
    /// (see AGENTS.md).
    pub database_url: String,
}

impl Config {
    /// Loads configuration from the environment, falling back to local defaults.
    pub fn from_env() -> Self {
        let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:data/app.db".into());
        Self {
            bind_addr,
            database_url,
        }
    }
}
