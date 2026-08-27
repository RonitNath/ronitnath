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
    ///
    /// A mounted file wins over the value: `RN_SITE__ID_KEY_FILE` names a path
    /// and is read at load, which is how Compose hands a secret in without it
    /// appearing in `docker inspect` or an env dump — the same contract
    /// [`crate::db::cluster`] already uses for the two raft secrets.
    pub id_key: Option<String>,
    /// The address that becomes this deployment's first platform operator the
    /// moment it registers, if it has not got one.
    ///
    /// `bootstrap-operator` opens the database directly, and hiqlite holds an
    /// exclusive lock on its data directory — so on a formed cluster there is
    /// no moment at which the subcommand can run: every voter is live. This is
    /// the same grant, taken by the running process (see [`crate::bootstrap`]).
    #[serde(default)]
    pub bootstrap_operator_email: Option<String>,
    /// This deployment's public origin — the scheme, host and port a reader
    /// types, with no trailing slash.
    ///
    /// It is the OpenID Provider's `iss`, *exactly*: every relying party
    /// compares it byte for byte, and every endpoint URL in the discovery
    /// document is built from it. Required in production for that reason —
    /// a deployment that guessed its own name would issue tokens nobody
    /// accepts — and loopback in dev, which is where `just run-dev` serves.
    #[serde(default = "default_public_origin")]
    pub public_origin: String,
    /// How this deployment names itself to a reader: the word at the top of
    /// the consent page and the sign-out confirmation.
    ///
    /// From config rather than a constant, because the OpenID Provider is a
    /// feature of the platform and not of this site: a deployment that is not
    /// ronitnath.com says its own name. Absent, the issuer's host stands in,
    /// which is at least true.
    #[serde(default)]
    pub public_name: Option<String>,
    /// The key the OpenID Provider's signing keys are sealed under at rest,
    /// 64 lowercase hex characters (an AES-256 key).
    ///
    /// Not the id key, and deliberately a second variable: the id key names
    /// rows, and this one forges identity. One secret that did both would be
    /// one leak that did both. `RN_SITE__OIDC_KEY_FILE` names a file holding
    /// it, the same contract the id key and the raft secrets already use.
    ///
    /// Required in prod. In dev an ephemeral one is minted per process, which
    /// means the signing keys already in the database will not open — so a dev
    /// process rotates a fresh one on demand and the JWKS changes per boot,
    /// which is what a disposable-dev database is.
    pub oidc_key: Option<String>,
    /// Whether this process serves the developer sign-in bypass
    /// (`RN_SITE__DEV=1`, and `crates/server/src/auth/dev.rs`).
    ///
    /// The flag is only half the gate and the weaker half: the route, its
    /// handler and the kernel entry behind it are all `#[cfg(debug_assertions)]`,
    /// so a release binary has nothing for this to turn on. It is read here
    /// rather than under the same `cfg` because a configuration field that
    /// exists in one profile and not the other is a struct that two halves of
    /// the tree spell differently.
    #[serde(default)]
    pub dev: bool,
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
            .set_default("dev", false)?
            .set_default("public_origin", default_public_origin())?
            .add_source(config::File::with_name(&path).required(false))
            .add_source(
                config::Environment::with_prefix("RN_SITE")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;
        let mut loaded: Self = settings.try_deserialize()?;
        if let Some(from_file) = id_key_file()? {
            loaded.id_key = Some(from_file);
        }
        if let Some(from_file) = secret_file("RN_SITE__OIDC_KEY_FILE")? {
            loaded.oidc_key = Some(from_file);
        }
        loaded.public_origin = loaded.public_origin.trim_end_matches('/').to_owned();
        loaded.validate()?;
        Ok(loaded)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        match &self.oidc_key {
            Some(key) if !is_seal_key(key) => {
                return Err(ConfigError::Invalid(
                    "oidc_key must be 64 lowercase hex characters (an AES-256 key)".into(),
                ));
            }
            None if self.mode == Mode::Prod => {
                return Err(ConfigError::Invalid(
                    "mode=prod requires RN_SITE__OIDC_KEY: the OpenID Provider's signing keys \
                     are sealed under it, so a generated one would make every key in the \
                     database unreadable on restart"
                        .into(),
                ));
            }
            _ => {}
        }
        if self.mode == Mode::Prod && !self.public_origin.starts_with("https://") {
            return Err(ConfigError::Invalid(
                "mode=prod requires an https RN_SITE__PUBLIC_ORIGIN: it is the OpenID \
                 Provider's issuer, and every relying party compares it byte for byte"
                    .into(),
            ));
        }
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
    /// What a page calls this deployment: the configured name, or the
    /// issuer's host.
    #[must_use]
    pub fn display_name(&self) -> String {
        if let Some(name) = self.public_name.as_deref().map(str::trim)
            && !name.is_empty()
        {
            return name.to_owned();
        }
        self.public_origin
            .split("://")
            .nth(1)
            .unwrap_or(&self.public_origin)
            .to_owned()
    }

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

/// The OpenID Provider, built from this configuration.
///
/// The sealing key is minted when none is configured, which
/// [`Self::validate`] has already made impossible in production.
impl AppConfig {
    #[must_use]
    pub fn provider(&self) -> rn_kernel::oidc::Provider {
        let seal = self
            .oidc_key
            .as_deref()
            .and_then(rn_kernel::oidc::SealKey::from_hex)
            .unwrap_or_else(|| {
                tracing::warn!(
                    "no oidc_key configured; minting an ephemeral one for this dev process — \
                     signing keys already in the database will not open"
                );
                rn_kernel::oidc::SealKey::mint()
            });
        rn_kernel::oidc::Provider::new(self.public_origin.clone(), seal)
    }
}

/// Where this deployment answers, when nothing says otherwise: the address
/// `just run-dev` serves on.
fn default_public_origin() -> String {
    "http://127.0.0.1:3004".to_owned()
}

fn is_seal_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// The id key from the file `RN_SITE__ID_KEY_FILE` names, if it names one.
///
/// Trailing whitespace is trimmed before anything looks at the value: a secret
/// written by `printf` and a secret written by an editor differ by one newline,
/// and a 33-character key is refused by [`is_id_key`] with a message about hex
/// digits that is true and useless.
fn id_key_file() -> Result<Option<String>, ConfigError> {
    secret_file("RN_SITE__ID_KEY_FILE")
}

/// A secret from the file a variable names, trimmed of the newline an editor
/// leaves and `printf` does not.
fn secret_file(variable: &str) -> Result<Option<String>, ConfigError> {
    let Ok(path) = std::env::var(variable) else {
        return Ok(None);
    };
    let path = path.trim();
    if path.is_empty() {
        return Ok(None);
    }
    let key = std::fs::read_to_string(path).map_err(|error| {
        ConfigError::Invalid(format!("{variable}: cannot read {path}: {error}"))
    })?;
    Ok(Some(key.trim_end().to_string()))
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
            bootstrap_operator_email: None,
            public_origin: "https://ronitnath.com".to_owned(),
            public_name: None,
            oidc_key: Some("a".repeat(64)),
            dev: false,
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

    /// Everything `RN_SITE__ID_KEY_FILE` promises, in one test because they all
    /// move the same process-wide variable and two tests that did would race.
    ///
    /// A file wins over the value; a trailing newline is not part of the key —
    /// a secret written by `printf` and one written by an editor differ by
    /// exactly that, and the second would otherwise be refused with a message
    /// about hex digits; an unset or empty path is absent rather than an error,
    /// which is how Compose spells "not this deployment"; and a path that was
    /// named and cannot be read is a boot failure rather than a quietly
    /// generated key, which would rename every object on the deployment.
    #[test]
    fn a_mounted_key_file_wins_over_the_variable_and_loses_its_newline() {
        // SAFETY: the whole contract is exercised here rather than across
        // tests precisely so nothing else is reading this variable meanwhile.
        unsafe { std::env::remove_var("RN_SITE__ID_KEY_FILE") };
        assert!(id_key_file().expect("unset is not an error").is_none());

        unsafe { std::env::set_var("RN_SITE__ID_KEY_FILE", "   ") };
        assert!(id_key_file().expect("empty is not an error").is_none());

        unsafe { std::env::set_var("RN_SITE__ID_KEY_FILE", "/nowhere/rn-site/id.key") };
        assert!(id_key_file().is_err(), "a named path that cannot be read");

        let dir = std::env::temp_dir().join(format!("rn-site-id-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let path = dir.join("id.key");
        let key = "0123456789abcdef0123456789abcdef";
        std::fs::write(&path, format!("{key}\n")).expect("the key file");

        unsafe { std::env::set_var("RN_SITE__ID_KEY_FILE", &path) };
        let read = id_key_file().expect("the file reads");
        unsafe { std::env::remove_var("RN_SITE__ID_KEY_FILE") };
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(read.as_deref(), Some(key), "the newline is not the key");
        assert!(is_id_key(read.as_deref().unwrap()), "and it validates");
    }

    /// `RN_SITE__DEV=1` is what the `.env` says, and `1` is not the word
    /// `true`: the environment source parses it as an integer, so the flag is
    /// only a flag if the deserializer converts one. Built through the same
    /// builder [`AppConfig::load`] uses, without touching the process
    /// environment, which two tests running at once would race over.
    #[test]
    fn the_dev_flag_is_off_by_default_and_the_number_one_turns_it_on() {
        let build = |dev: Option<config::Value>| {
            let mut builder = config::Config::builder()
                .set_default("mode", "dev")
                .expect("a default")
                .set_default("db_path", "data/db.sqlite")
                .expect("a default")
                .set_default("addr", "127.0.0.1:3004")
                .expect("a default")
                .set_default("static_dir", "static")
                .expect("a default")
                .set_default("dev", false)
                .expect("a default");
            if let Some(dev) = dev {
                builder = builder.set_override("dev", dev).expect("an override");
            }
            builder
                .build()
                .expect("a configuration")
                .try_deserialize::<AppConfig>()
                .expect("it deserializes")
        };
        assert!(!build(None).dev, "the checked-in default is off");
        assert!(build(Some(1.into())).dev, "RN_SITE__DEV=1");
        assert!(build(Some(true.into())).dev, "RN_SITE__DEV=true");
        assert!(!build(Some(0.into())).dev);
    }

    #[test]
    fn a_bare_filename_keeps_its_database_in_the_working_directory() {
        let mut cfg = cfg(Mode::Dev, None);
        cfg.db_path = PathBuf::from("db.sqlite");
        assert_eq!(cfg.db_dir(), PathBuf::from("."));
    }
}
