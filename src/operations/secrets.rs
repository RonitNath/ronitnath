//! Durable secrets loaded from `.env` (and written back when missing).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Env / `.env` key for the session cookie signing secret.
pub const COOKIE_SECRET_KEY: &str = "COOKIE_SECRET";

const DEFAULT_ENV_PATH: &str = ".env";

/// Ensure [`COOKIE_SECRET_KEY`] is available for this process and persisted in
/// `.env` so sessions survive restarts.
///
/// Order:
/// 1. Load `.env` into the process environment (if the file exists).
/// 2. Prefer the value already in the `.env` file.
/// 3. Else use a non-empty process env value, and write it into `.env`.
/// 4. Else generate a new secret, write it into `.env`, and export it.
#[instrument(
    name = "secrets.ensure_cookie_secret",
    skip_all,
    fields(env_path = %env_path.as_ref().display())
)]
pub fn ensure_cookie_secret(env_path: impl AsRef<Path>) -> Result<String, SecretError> {
    let env_path = env_path.as_ref();

    // Populate process env from file without overriding vars already set.
    match dotenvy::from_path(env_path) {
        Ok(_) => {}
        Err(dotenvy::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(SecretError::Dotenv(err)),
    }

    let from_file = read_env_file_value(env_path, COOKIE_SECRET_KEY)?;
    if let Some(secret) = from_file.filter(|s| !s.is_empty()) {
        // Keep process env aligned with the durable file.
        // SAFETY: single-threaded at startup before worker threads use env.
        unsafe { std::env::set_var(COOKIE_SECRET_KEY, &secret) };
        info!("loaded cookie secret from .env");
        return Ok(secret);
    }

    let from_env = std::env::var(COOKIE_SECRET_KEY)
        .ok()
        .filter(|s| !s.is_empty());
    let secret = match from_env {
        Some(secret) => {
            warn!("COOKIE_SECRET set in environment but missing from .env; persisting");
            secret
        }
        None => {
            let secret = generate_cookie_secret();
            info!("generated new cookie secret");
            secret
        }
    };

    upsert_env_file_value(env_path, COOKIE_SECRET_KEY, &secret)?;
    unsafe { std::env::set_var(COOKIE_SECRET_KEY, &secret) };
    info!(path = %env_path.display(), "persisted cookie secret to .env");
    Ok(secret)
}

/// Default path: project-root `.env`.
pub fn ensure_cookie_secret_default() -> Result<String, SecretError> {
    ensure_cookie_secret(DEFAULT_ENV_PATH)
}

fn generate_cookie_secret() -> String {
    // 32 bytes of entropy as hex (64 chars) — enough for HMAC cookie signing.
    format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn read_env_file_value(path: &Path, key: &str) -> Result<Option<String>, SecretError> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(None);
    };
    Ok(parse_env_value(&contents, key))
}

fn parse_env_value(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(raw) = trimmed.strip_prefix(&prefix) {
            let value = strip_env_quotes(raw.trim());
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn strip_env_quotes(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn upsert_env_file_value(path: &Path, key: &str, value: &str) -> Result<(), SecretError> {
    let line = format!("{key}={value}");
    match fs::read_to_string(path) {
        Ok(contents) => {
            let prefix = format!("{key}=");
            let mut replaced = false;
            let mut out = String::with_capacity(contents.len() + line.len() + 1);
            for (idx, existing) in contents.lines().enumerate() {
                if idx > 0 {
                    out.push('\n');
                }
                let trimmed = existing.trim_start();
                if !replaced && trimmed.starts_with(&prefix) && !trimmed.starts_with('#') {
                    out.push_str(&line);
                    replaced = true;
                } else {
                    out.push_str(existing);
                }
            }
            if !replaced {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&line);
                out.push('\n');
            } else if contents.ends_with('\n') && !out.ends_with('\n') {
                out.push('\n');
            }
            fs::write(path, out).map_err(|source| SecretError::Io {
                path: path.display().to_string(),
                source,
            })?;
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .map_err(|source| SecretError::Io {
                    path: path.display().to_string(),
                    source,
                })?;
            writeln!(file, "{line}").map_err(|source| SecretError::Io {
                path: path.display().to_string(),
                source,
            })?;
        }
        Err(source) => {
            return Err(SecretError::Io {
                path: path.display().to_string(),
                source,
            });
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error(transparent)]
    Dotenv(#[from] dotenvy::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // serialize tests that touch process env
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn generates_and_persists_when_missing() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        unsafe { std::env::remove_var(COOKIE_SECRET_KEY) };

        let secret = ensure_cookie_secret(&path).unwrap();
        assert_eq!(secret.len(), 64);
        assert_eq!(std::env::var(COOKIE_SECRET_KEY).unwrap(), secret);

        let file = fs::read_to_string(&path).unwrap();
        assert!(file.contains(&format!("{COOKIE_SECRET_KEY}={secret}")));

        // Second boot reuses the same value.
        let again = ensure_cookie_secret(&path).unwrap();
        assert_eq!(again, secret);
    }

    #[test]
    fn prefers_existing_env_file_value() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        fs::write(&path, "COOKIE_SECRET=already-set-from-file-value-ok\nOTHER=1\n").unwrap();
        unsafe { std::env::remove_var(COOKIE_SECRET_KEY) };

        let secret = ensure_cookie_secret(&path).unwrap();
        assert_eq!(secret, "already-set-from-file-value-ok");
        let file = fs::read_to_string(&path).unwrap();
        assert!(file.contains("OTHER=1"));
    }

    #[test]
    fn persists_process_env_value_into_file() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        fs::write(&path, "OTHER=keep-me\n").unwrap();
        unsafe { std::env::set_var(COOKIE_SECRET_KEY, "from-process-env-secret-value") };

        let secret = ensure_cookie_secret(&path).unwrap();
        assert_eq!(secret, "from-process-env-secret-value");
        let file = fs::read_to_string(&path).unwrap();
        assert!(file.contains("OTHER=keep-me"));
        assert!(file.contains("COOKIE_SECRET=from-process-env-secret-value"));

        unsafe { std::env::remove_var(COOKIE_SECRET_KEY) };
    }
}
