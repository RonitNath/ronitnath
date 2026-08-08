//! App config via the [`config`](https://docs.rs/config) crate.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Runtime mode. `dev` is the checked-in default for `cargo run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Dev,
    Prod,
}

impl Mode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Prod => "prod",
        }
    }
}

/// Process configuration loaded from defaults → `config.toml` → env.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub mode: Mode,
    /// Path to the SQLite state-machine file (parent dir is the hiqlite data dir).
    pub db_path: PathBuf,
}

impl AppConfig {
    /// Defaults matching the checked-in `config.toml`.
    pub fn load() -> Result<Self, config::ConfigError> {
        let config_path = std::env::var("RN_SITE_CONFIG").unwrap_or_else(|_| "config".to_string());

        let settings = config::Config::builder()
            .set_default("mode", "dev")?
            .set_default("db_path", "data/db.sqlite")?
            .add_source(config::File::with_name(&config_path).required(false))
            .add_source(
                config::Environment::with_prefix("RN_SITE")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        settings.try_deserialize()
    }

    /// Directory that must exist for hiqlite (`data/` for the default path).
    #[must_use]
    pub fn db_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// SQLite filename inside [`Self::db_dir`] (`db.sqlite` for the default path).
    #[must_use]
    pub fn db_filename(&self) -> String {
        self.db_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("db.sqlite")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_checked_in_toml() {
        // No file required — builder defaults alone.
        let cfg = config::Config::builder()
            .set_default("mode", "dev")
            .unwrap()
            .set_default("db_path", "data/db.sqlite")
            .unwrap()
            .build()
            .unwrap()
            .try_deserialize::<AppConfig>()
            .unwrap();
        assert_eq!(cfg.mode, Mode::Dev);
        assert_eq!(cfg.db_path, PathBuf::from("data/db.sqlite"));
        assert_eq!(cfg.db_dir(), PathBuf::from("data"));
        assert_eq!(cfg.db_filename(), "db.sqlite");
    }
}
