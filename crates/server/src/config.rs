//! Process configuration: defaults → `config.toml` → `RN_SITE__*` environment.
//!
//! The prefix and the `__` separator are the deployed contract (deploy/
//! compose.yaml sets `RN_SITE__MODE` and `RN_SITE__DB_PATH`; the Containerfile
//! adds `RN_SITE__ADDR` and `RN_SITE__STATIC_DIR`). The cluster topology is
//! *not* here: it arrives as the flat `RN_SITE_HQL_*` variables the provisioned
//! `node.env` writes, and is parsed in [`crate::db::cluster`].

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Runtime mode. `dev` is the checked-in default so `cargo run` just works.
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

/// Everything the process needs before it can open a socket or a database.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub mode: Mode,
    /// Logical SQLite filename whose parent is hiqlite's complete data dir.
    /// hiqlite places the actual state-machine file beneath that directory.
    pub db_path: PathBuf,
    /// Listener for the HTTP surface.
    pub addr: SocketAddr,
    /// Root of the as-is asset tree, used when assets are served from disk.
    pub static_dir: PathBuf,
    /// AES-128 key for public-id derivation, 32 lowercase hex characters.
    /// Required in prod; minted per boot in dev (see [`Self::require_id_key`]).
    pub id_key: Option<String>,
}

/// Everything that can go wrong turning the environment into an [`AppConfig`].
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    Source(#[from] config::ConfigError),
    #[error("{0}")]
    Invalid(String),
}

impl AppConfig {
    /// Load defaults, then `config.toml` (or `RN_SITE_CONFIG`), then the
    /// environment. Validation that depends on the mode runs here so a bad
    /// production process fails at boot rather than at first request.
    pub fn load() -> Result<Self, ConfigError> {
        let path = std::env::var("RN_SITE_CONFIG").unwrap_or_else(|_| "config".to_string());
        let settings = config::Config::builder()
            .set_default("mode", "dev")?
            .set_default("db_path", "data/db.sqlite")?
            .set_default("addr", "127.0.0.1:3004")?
            .set_default("static_dir", "static")?
            .add_source(config::File::with_name(&path).required(false))
            .add_source(
                config::Environment::with_prefix("RN_SITE")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;
        let loaded: Self = settings.try_deserialize()?;
        loaded.validate()?;
        Ok(loaded)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        match &self.id_key {
            Some(key) if !is_id_key(key) => Err(ConfigError::Invalid(
                "id_key must be 32 lowercase hex characters (an AES-128 key)".into(),
            )),
            None if self.mode == Mode::Prod => Err(ConfigError::Invalid(
                "mode=prod requires RN_SITE__ID_KEY: public ids are derived from it, so a \
                 generated one would rename every object on restart"
                    .into(),
            )),
            _ => Ok(()),
        }
    }

    /// The configured key, or — in dev only — a fresh one, reported as absent.
    ///
    /// Returning the generated value rather than storing it keeps the dev
    /// behaviour honest: ids are stable for the life of the process and not
    /// across restarts, which is exactly what a disposable-dev database is.
    #[must_use]
    pub fn require_id_key(&self) -> String {
        match &self.id_key {
            Some(key) => key.clone(),
            None => {
                let generated = generated_id_key();
                tracing::warn!(
                    "no id_key configured; minting an ephemeral one for this dev process — \
                     public ids will not survive a restart"
                );
                generated
            }
        }
    }

    /// Directory that must exist for hiqlite (`data/` for the default path).
    #[must_use]
    pub fn db_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// SQLite filename inside [`Self::db_dir`] (`db.sqlite` by default).
    #[must_use]
    pub fn db_filename(&self) -> String {
        self.db_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("db.sqlite")
            .to_string()
    }
}

fn is_id_key(key: &str) -> bool {
    key.len() == 32
        && key
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// A process-lifetime key with no dependency on a CSPRNG crate: the address of
/// a heap allocation and the boot instant, hashed pairwise. Dev-only by
/// construction — [`AppConfig::validate`] refuses to reach here in prod.
fn generated_id_key() -> String {
    use std::hash::{BuildHasher, RandomState};
    let seed = RandomState::new();
    let mut out = String::with_capacity(32);
    for salt in 0..2u64 {
        let word = seed.hash_one(salt ^ 0x9e37_79b9_7f4a_7c15);
        out.push_str(&format!("{word:016x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: Mode, id_key: Option<&str>) -> AppConfig {
        AppConfig {
            mode,
            db_path: PathBuf::from("data/db.sqlite"),
            addr: "127.0.0.1:3004".parse().unwrap(),
            static_dir: PathBuf::from("static"),
            id_key: id_key.map(str::to_string),
        }
    }

    #[test]
    fn the_checked_in_defaults_are_the_ones_a_bare_cargo_run_gets() {
        let cfg = cfg(Mode::Dev, None);
        assert_eq!(cfg.db_dir(), PathBuf::from("data"));
        assert_eq!(cfg.db_filename(), "db.sqlite");
        assert_eq!(cfg.mode.as_str(), "dev");
    }

    #[test]
    fn production_refuses_to_boot_without_an_id_key() {
        assert!(cfg(Mode::Prod, None).validate().is_err());
        assert!(cfg(Mode::Prod, Some(&"a".repeat(32))).validate().is_ok());
    }

    #[test]
    fn a_malformed_id_key_is_refused_in_either_mode() {
        for mode in [Mode::Dev, Mode::Prod] {
            assert!(cfg(mode, Some("too-short")).validate().is_err());
            assert!(cfg(mode, Some(&"A".repeat(32))).validate().is_err());
            assert!(cfg(mode, Some(&"z".repeat(32))).validate().is_err());
        }
    }

    #[test]
    fn dev_mints_a_well_formed_ephemeral_key_when_none_is_configured() {
        let key = cfg(Mode::Dev, None).require_id_key();
        assert!(is_id_key(&key), "{key}");
        assert_ne!(key, cfg(Mode::Dev, None).require_id_key());
    }

    #[test]
    fn a_bare_filename_keeps_its_database_in_the_working_directory() {
        let mut cfg = cfg(Mode::Dev, None);
        cfg.db_path = PathBuf::from("db.sqlite");
        assert_eq!(cfg.db_dir(), PathBuf::from("."));
    }
}
