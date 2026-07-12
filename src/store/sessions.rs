//! Queries and types for the `sessions` table.
//!
//! A session pins an identity to its currently active account. Cookie
//! plumbing (generating/hashing the raw token, reading/writing the cookie)
//! lives in [`crate::auth::session`]; this module only ever sees the
//! already-hashed value.

use super::Store;

/// The result of resolving a session cookie: everything a request needs to
/// know who's asking and what they're allowed to touch. Re-derived fresh on
/// every request (see [`Store::find_session_context`]) rather than cached,
/// so a revoked membership or session takes effect on the very next
/// request, not at next login.
#[derive(Debug, Clone)]
pub struct SessionContext {
    pub session_id: i64,
    pub csrf_token: String,
    pub identity_id: i64,
    pub display_name: String,
    pub account_id: i64,
    pub account_name: String,
    pub account_purpose: String,
    pub role: String,
}

pub struct SessionSummary {
    pub id: i64,
    pub created_at: String,
    pub last_seen_at: String,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    pub is_current: bool,
}

impl Store {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_session(
        &self,
        identity_id: i64,
        account_id: i64,
        token_hash: &str,
        csrf_token: &str,
        expires_at: &str,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> sqlx::Result<i64> {
        let row = sqlx::query!(
            r#"INSERT INTO sessions
                   (identity_id, account_id, token_hash, csrf_token, expires_at, user_agent, ip)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
               RETURNING id as "id!: i64""#,
            identity_id,
            account_id,
            token_hash,
            csrf_token,
            expires_at,
            user_agent,
            ip,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.id)
    }

    /// `None` if the token doesn't match a session, the session is
    /// revoked/expired, or the membership backing it no longer exists (the
    /// join requires a live `memberships` row) — all three read as "not
    /// authenticated" to the caller.
    pub async fn find_session_context(
        &self,
        token_hash: &str,
    ) -> sqlx::Result<Option<SessionContext>> {
        sqlx::query_as!(
            SessionContext,
            r#"SELECT s.id as "session_id: i64", s.csrf_token,
                      s.identity_id as "identity_id: i64", i.display_name,
                      s.account_id as "account_id: i64", a.name as account_name,
                      a.purpose as account_purpose, m.role
               FROM sessions s
               JOIN identities i ON i.id = s.identity_id AND i.deleted_at IS NULL
               JOIN accounts a ON a.id = s.account_id AND a.deleted_at IS NULL
               JOIN memberships m ON m.identity_id = s.identity_id AND m.account_id = s.account_id
               WHERE s.token_hash = ?1
                 AND s.revoked_at IS NULL
                 AND s.expires_at > datetime('now')"#,
            token_hash,
        )
        .fetch_optional(&self.pool)
        .await
    }

    /// Advances `last_seen_at` — call at most once/minute per session
    /// (checked by the caller) to avoid a write on every single request.
    pub async fn touch_session(&self, session_id: i64) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE sessions SET last_seen_at = datetime('now') WHERE id = ?1",
            session_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_sessions(
        &self,
        identity_id: i64,
        current_session_id: i64,
    ) -> sqlx::Result<Vec<SessionSummary>> {
        let rows = sqlx::query!(
            r#"SELECT id as "id: i64", created_at, last_seen_at, user_agent, ip
               FROM sessions
               WHERE identity_id = ?1 AND revoked_at IS NULL AND expires_at > datetime('now')
               ORDER BY last_seen_at DESC"#,
            identity_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SessionSummary {
                is_current: r.id == current_session_id,
                id: r.id,
                created_at: r.created_at,
                last_seen_at: r.last_seen_at,
                user_agent: r.user_agent,
                ip: r.ip,
            })
            .collect())
    }

    /// Revokes a session owned by `identity_id` (ownership check prevents
    /// revoking someone else's session by guessing an id).
    pub async fn revoke_session(&self, id: i64, identity_id: i64) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE sessions SET revoked_at = datetime('now') WHERE id = ?1 AND identity_id = ?2",
            id,
            identity_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
