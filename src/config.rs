use anyhow::Context as _;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) domain: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_owned(),
            port: 8080,
            domain: "ronitnath.com".to_owned(),
        }
    }
}

impl Config {
    pub(crate) fn load() -> anyhow::Result<Self> {
        let mut cfg: Self = match std::fs::read_to_string("config.toml") {
            Ok(raw) => toml::from_str(&raw).context("parse config.toml")?,
            Err(_) => Self::default(),
        };
        if let Ok(v) = std::env::var("HOST") {
            cfg.host = v;
        }
        if let Ok(v) = std::env::var("PORT") {
            cfg.port = v.parse().with_context(|| format!("invalid PORT={v}"))?;
        }
        if let Ok(v) = std::env::var("DOMAIN") {
            cfg.domain = v;
        }
        Ok(cfg)
    }
}
