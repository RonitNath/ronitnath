//! Queries and types for the `memberships` table.
//!
//! A membership is the edge `(identity_id, account_id, role)` — role lives
//! here and nowhere else, since "is X an admin" is only well-formed
//! per-account. [`Store::find_primary_membership`] (bearer-token auth) and
//! the join inside [`crate::store::sessions::find_session_context`]
//! (cookie auth) are the two places role gets read fresh on every
//! request, so a revoked membership takes effect immediately.

use super::Store;

pub struct PrimaryMembership {
    pub account_id: i64,
    pub account_name: String,
    pub role: String,
}

impl Store {
    /// Every phase-1 identity has exactly one membership (its auto-created
    /// personal account — there's no org/invite flow yet to create a
    /// second), so "pick one" is unambiguous. Used by bearer-token auth,
    /// which has no session to say which account is "active". Once
    /// multi-account identities exist (phase 2), api tokens will need to
    /// be minted per-account instead of per-identity.
    pub async fn find_primary_membership(&self, identity_id: i64) -> sqlx::Result<Option<PrimaryMembership>> {
        sqlx::query_as!(
            PrimaryMembership,
            r#"SELECT m.account_id as "account_id: i64", a.name as account_name, m.role
               FROM memberships m
               JOIN accounts a ON a.id = m.account_id AND a.deleted_at IS NULL
               WHERE m.identity_id = ?1
               ORDER BY m.id
               LIMIT 1"#,
            identity_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    /// No production caller yet — memberships are only ever created by
    /// [`Store::signup_with_password`] (as `owner`) until phase 2 adds
    /// invites. Exists so tests can seed a second identity onto an
    /// existing account with an arbitrary role (see the role-gating
    /// exemplar test in `app.rs`).
    #[cfg(test)]
    pub async fn create_membership(&self, identity_id: i64, account_id: i64, role: &str) -> sqlx::Result<()> {
        sqlx::query!(
            "INSERT INTO memberships (identity_id, account_id, role) VALUES (?1, ?2, ?3)",
            identity_id,
            account_id,
            role,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
