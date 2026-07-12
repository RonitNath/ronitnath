//! Claimed guest identities: the active edge from an owner-account person to a guest identity.

use super::Store;
use crate::store::factors::Factor;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GuestBinding {
    pub owner_account_id: i64,
    pub person_id: i64,
    pub person_name: String,
    pub identity_id: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ClaimStatus {
    pub identity_id: i64,
    pub display_name: String,
    pub claimed_at: String,
    pub factor_count: i64,
}

impl Store {
    pub async fn active_guest_binding(
        &self,
        identity_id: i64,
    ) -> sqlx::Result<Option<GuestBinding>> {
        sqlx::query_as!(GuestBinding,
            r#"SELECT pil.account_id as "owner_account_id!: i64", pil.person_id as "person_id!: i64",
                      p.name as person_name, pil.identity_id as "identity_id!: i64"
               FROM person_identity_links pil
               JOIN people p ON p.account_id = pil.account_id AND p.id = pil.person_id
               WHERE pil.identity_id = ?1 AND pil.unlinked_at IS NULL"#,
            identity_id)
            .fetch_optional(&self.pool).await
    }

    pub async fn active_identity_for_person(
        &self,
        account_id: i64,
        person_id: i64,
    ) -> sqlx::Result<Option<i64>> {
        sqlx::query_scalar!(
            r#"SELECT identity_id as "identity_id!: i64"
            FROM person_identity_links
            WHERE account_id = ?1 AND person_id = ?2 AND unlinked_at IS NULL"#,
            account_id,
            person_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn claim_guest(
        &self,
        owner_account_id: i64,
        person_id: i64,
        display_name: &str,
        recovery_email: Option<&str>,
        password_hash: &str,
        session_hash: &str,
        csrf_token: &str,
        expires_at: &str,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> sqlx::Result<(i64, i64)> {
        let mut tx = self.pool.begin().await?;
        // Re-check inside the write transaction: the partial unique index is the final race guard.
        let person_exists = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!: i64" FROM people p
            WHERE p.account_id = ?1 AND p.id = ?2
              AND NOT EXISTS (SELECT 1 FROM person_identity_links pil
                              WHERE pil.person_id = p.id AND pil.unlinked_at IS NULL)"#,
            owner_account_id,
            person_id
        )
        .fetch_one(&mut *tx)
        .await?;
        if person_exists != 1 {
            return Err(sqlx::Error::RowNotFound);
        }

        let identity_id = sqlx::query_scalar!(
            r#"INSERT INTO identities (kind, display_name)
            VALUES ('human', ?1) RETURNING id as "id!: i64""#,
            display_name
        )
        .fetch_one(&mut *tx)
        .await?;
        let guest_account_id = sqlx::query_scalar!(
            r#"INSERT INTO accounts (name, kind, purpose)
            VALUES (?1, 'personal', 'guest') RETURNING id as "id!: i64""#,
            display_name
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query!(
            "INSERT INTO memberships (identity_id, account_id, role) VALUES (?1, ?2, 'owner')",
            identity_id,
            guest_account_id
        )
        .execute(&mut *tx)
        .await?;
        let external_id = format!("guest:{person_id}");
        sqlx::query!(
            r#"INSERT INTO factors (identity_id, kind, external_id, secret_hash)
            VALUES (?1, 'password', ?2, ?3)"#,
            identity_id,
            external_id,
            password_hash
        )
        .execute(&mut *tx)
        .await?;
        if let Some(email) = recovery_email {
            sqlx::query!(
                "UPDATE people SET recovery_email = ?3 WHERE account_id = ?1 AND id = ?2",
                owner_account_id,
                person_id,
                email
            )
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query!(
            r#"INSERT INTO person_identity_links (account_id, person_id, identity_id)
            VALUES (?1, ?2, ?3)"#,
            owner_account_id,
            person_id,
            identity_id
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            r#"INSERT INTO sessions
            (identity_id, account_id, token_hash, csrf_token, expires_at, user_agent, ip)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            identity_id,
            guest_account_id,
            session_hash,
            csrf_token,
            expires_at,
            user_agent,
            ip
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok((identity_id, guest_account_id))
    }

    /// Recovery email is a locator, never the password factor external id. Ambiguity fails closed.
    pub async fn find_guest_password_by_email(&self, email: &str) -> sqlx::Result<Option<Factor>> {
        let rows = sqlx::query_as!(
            Factor,
            r#"SELECT f.id as "id: i64", f.identity_id as "identity_id: i64", f.kind,
                      f.external_id, f.secret_hash, f.created_at, f.last_used_at
               FROM people p
               JOIN person_identity_links pil ON pil.account_id = p.account_id
                    AND pil.person_id = p.id AND pil.unlinked_at IS NULL
               JOIN factors f ON f.identity_id = pil.identity_id AND f.kind = 'password'
                    AND f.external_id = 'guest:' || p.id
               WHERE lower(p.recovery_email) = ?1
               ORDER BY f.id LIMIT 2"#,
            email
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(match rows.len() {
            1 => rows.into_iter().next(),
            _ => None,
        })
    }

    pub async fn claim_status(
        &self,
        account_id: i64,
        person_id: i64,
    ) -> sqlx::Result<Option<ClaimStatus>> {
        sqlx::query_as!(
            ClaimStatus,
            r#"SELECT pil.identity_id as "identity_id!: i64", i.display_name, pil.claimed_at,
                      COUNT(f.id) as "factor_count!: i64"
               FROM person_identity_links pil
               JOIN identities i ON i.id = pil.identity_id
               LEFT JOIN factors f ON f.identity_id = pil.identity_id
               WHERE pil.account_id = ?1 AND pil.person_id = ?2 AND pil.unlinked_at IS NULL
               GROUP BY pil.id"#,
            account_id,
            person_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn force_unlink_guest(
        &self,
        account_id: i64,
        person_id: i64,
    ) -> sqlx::Result<Option<i64>> {
        let mut tx = self.pool.begin().await?;
        let identity_id = sqlx::query_scalar!(
            r#"UPDATE person_identity_links SET unlinked_at = datetime('now')
            WHERE account_id = ?1 AND person_id = ?2 AND unlinked_at IS NULL
            RETURNING identity_id as "identity_id!: i64""#,
            account_id,
            person_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(identity_id) = identity_id {
            // `factors(kind, external_id)` is globally unique. Tombstone the old
            // synthetic handle so a later claim can faithfully reuse guest:{person_id}.
            let tombstone = format!("unlinked:{identity_id}:{person_id}");
            let active_handle = format!("guest:{person_id}");
            sqlx::query!(
                r#"UPDATE factors SET external_id = ?3
                WHERE identity_id = ?1 AND kind = 'password' AND external_id = ?2"#,
                identity_id,
                active_handle,
                tombstone
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(identity_id)
    }
}
