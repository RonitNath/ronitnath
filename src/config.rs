use anyhow::Context as _;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) domain: String,
    pub(crate) database_url: String,
    pub(crate) public_base_url: String,
    pub(crate) token_secret: String,
    pub(crate) isoastra: IsoastraConfig,
    pub(crate) session: SessionConfig,
    pub(crate) admins: AdminConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct IsoastraConfig {
    pub(crate) issuer: String,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
    pub(crate) redirect_uri: String,
}

const DEFAULT_ISOASTRA_CLIENT_SECRET: &str = "dev-only-change-me";
const DEFAULT_TOKEN_SECRET: &str = "dev-only-change-me-dev-only-change-me";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct SessionConfig {
    pub(crate) cookie_name: String,
    pub(crate) cookie_secure: bool,
    pub(crate) expiry_hours: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub(crate) struct AdminConfig {
    pub(crate) identity_ids: Vec<String>,
    pub(crate) emails: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_owned(),
            port: 8080,
            domain: "ronitnath.com".to_owned(),
            database_url: "sqlite://./data/ronitnath.db".to_owned(),
            public_base_url: "https://ronitnath.com".to_owned(),
            token_secret: DEFAULT_TOKEN_SECRET.to_owned(),
            isoastra: IsoastraConfig::default(),
            session: SessionConfig::default(),
            admins: AdminConfig::default(),
        }
    }
}

impl Default for IsoastraConfig {
    fn default() -> Self {
        Self {
            issuer: "https://auth.isoastra.com".to_owned(),
            client_id: "ronitnath".to_owned(),
            client_secret: DEFAULT_ISOASTRA_CLIENT_SECRET.to_owned(),
            redirect_uri: "https://ronitnath.com/auth/callback".to_owned(),
        }
    }
}

impl IsoastraConfig {
    pub(crate) fn is_configured(&self) -> bool {
        !self.issuer.trim().is_empty()
            && !self.client_id.trim().is_empty()
            && !self.redirect_uri.trim().is_empty()
            && !self.client_secret.trim().is_empty()
            && self.client_secret != DEFAULT_ISOASTRA_CLIENT_SECRET
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            cookie_name: "rn_sid".to_owned(),
            cookie_secure: true,
            expiry_hours: 24,
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
        if let Ok(v) = std::env::var("DATABASE_URL") {
            cfg.database_url = v;
        }
        if let Ok(v) = std::env::var("PUBLIC_BASE_URL") {
            cfg.public_base_url = v;
        }
        if let Ok(v) = std::env::var("EVENT_TOKEN_SECRET") {
            cfg.token_secret = v;
        }
        if let Ok(v) = std::env::var("ISOASTRA_ISSUER") {
            cfg.isoastra.issuer = v;
        }
        if let Ok(v) = std::env::var("ISOASTRA_CLIENT_ID") {
            cfg.isoastra.client_id = v;
        }
        if let Ok(v) = std::env::var("ISOASTRA_CLIENT_SECRET") {
            cfg.isoastra.client_secret = v;
        }
        if let Ok(v) = std::env::var("ISOASTRA_REDIRECT_URI") {
            cfg.isoastra.redirect_uri = v;
        }
        if let Ok(v) = std::env::var("SESSION_COOKIE_NAME") {
            cfg.session.cookie_name = v;
        }
        if let Ok(v) = std::env::var("SESSION_COOKIE_SECURE") {
            cfg.session.cookie_secure =
                parse_bool(&v).with_context(|| format!("invalid SESSION_COOKIE_SECURE={v}"))?;
        }
        if let Ok(v) = std::env::var("SESSION_EXPIRY_HOURS") {
            cfg.session.expiry_hours = v
                .parse()
                .with_context(|| format!("invalid SESSION_EXPIRY_HOURS={v}"))?;
        }
        if let Ok(v) = std::env::var("ADMIN_IDENTITY_IDS") {
            cfg.admins.identity_ids = parse_csv(&v);
        }
        if let Ok(v) = std::env::var("ADMIN_EMAILS") {
            cfg.admins.emails = parse_csv(&v);
        }
        Ok(cfg)
    }
}

fn parse_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => anyhow::bail!("expected boolean"),
    }
}
