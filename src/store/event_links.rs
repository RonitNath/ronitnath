//! Queries and types for the `event_links` table — capability links, the
//! only path a guest has to an event page.
//!
//! Guest lookups resolve by `token_hash` and are deliberately NOT
//! account-scoped: the token *is* the credential, and a guest has no
//! account. Every admin-facing query here is account-scoped as usual.

use rand_core::{OsRng, RngCore};
use serde::Serialize;

use super::Store;

/// Alphabet for personalized-token suffixes: lowercase alphanumerics minus
/// the lookalikes (i/l/1, o/0) so a link survives being read aloud or
/// hand-typed off a text message.
const SUFFIX_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

/// Mints a personalized invite token: the person's name slugified, then a
/// 4-char random suffix, dash-separated ("maya-k4x9"). The suffix is what
/// makes revoke-and-remint possible (same name, new link); the name is
/// what makes the URL feel like theirs. Deliberately low-entropy compared
/// to session tokens — it gates a party invite, not an account — with the
/// guest surface's rate limiter as the brute-force backstop.
pub fn personal_token(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() { "guest" } else { slug };

    let mut bytes = [0u8; 4];
    OsRng.fill_bytes(&mut bytes);
    let suffix: String = bytes
        .iter()
        .map(|b| SUFFIX_ALPHABET[(*b as usize) % SUFFIX_ALPHABET.len()] as char)
        .collect();
    format!("{slug}-{suffix}")
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EventLink {
    pub id: i64,
    pub event_id: i64,
    pub person_id: Option<i64>,
    pub token_plain: String,
    pub label: String,
    pub tier: String,
    pub revoked_at: Option<String>,
    pub uses: i64,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

/// A link row joined with the person's name for the admin table.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EventLinkRow {
    pub id: i64,
    pub person_id: Option<i64>,
    pub person_name: Option<String>,
    pub token_plain: String,
    pub label: String,
    pub tier: String,
    pub revoked_at: Option<String>,
    pub uses: i64,
    pub last_used_at: Option<String>,
}

/// What token resolution hands the guest handlers: the link plus which
/// account owns the event it points at (guest queries can't take an
/// account id from anywhere else).
#[derive(Debug, sqlx::FromRow)]
pub struct ResolvedLink {
    pub id: i64,
    pub account_id: i64,
    pub event_id: i64,
    pub person_id: Option<i64>,
    pub tier: String,
}

impl Store {
    pub async fn create_event_link(
        &self,
        account_id: i64,
        event_id: i64,
        person_id: Option<i64>,
        token_hash: &str,
        token_plain: &str,
        label: &str,
        tier: &str,
    ) -> sqlx::Result<i64> {
        sqlx::query_scalar!(
            r#"INSERT INTO event_links
                   (account_id, event_id, person_id, token_hash, token_plain, label, tier)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
               RETURNING id as "id!: i64""#,
            account_id,
            event_id,
            person_id,
            token_hash,
            token_plain,
            label,
            tier,
        )
        .fetch_one(&self.pool)
        .await
    }

    /// The resolver lookup without use-counter mutation. Import verification
    /// uses this exact production predicate while remaining read-only.
    pub async fn resolve_event_link_read_only(
        &self,
        token_hash: &str,
    ) -> sqlx::Result<Option<ResolvedLink>> {
        sqlx::query_as!(
            ResolvedLink,
            r#"SELECT id as "id!: i64", account_id as "account_id!: i64",
                      event_id as "event_id!: i64", person_id, tier
               FROM event_links
               WHERE token_hash = ?1 AND revoked_at IS NULL"#,
            token_hash,
        )
        .fetch_optional(&self.pool)
        .await
    }

    /// Resolves a guest's token to a live link, bumping the use counter.
    /// Returns `None` for unknown or revoked tokens — the two cases are
    /// indistinguishable on purpose.
    pub async fn resolve_event_link(&self, token_hash: &str) -> sqlx::Result<Option<ResolvedLink>> {
        let link = self.resolve_event_link_read_only(token_hash).await?;

        if let Some(link) = &link {
            sqlx::query!(
                "UPDATE event_links SET uses = uses + 1, last_used_at = datetime('now') WHERE id = ?1",
                link.id,
            )
            .execute(&self.pool)
            .await?;
        }
        Ok(link)
    }

    pub async fn list_event_links(
        &self,
        account_id: i64,
        event_id: i64,
    ) -> sqlx::Result<Vec<EventLinkRow>> {
        sqlx::query_as!(
            EventLinkRow,
            r#"SELECT l.id as "id!: i64", l.person_id, p.name as "person_name?: String",
                      l.token_plain, l.label, l.tier, l.revoked_at,
                      l.uses as "uses!: i64", l.last_used_at
               FROM event_links l
               LEFT JOIN people p ON p.account_id = l.account_id AND p.id = l.person_id
               WHERE l.account_id = ?1 AND l.event_id = ?2
               ORDER BY l.person_id IS NOT NULL, p.name COLLATE NOCASE, l.created_at"#,
            account_id,
            event_id,
        )
        .fetch_all(&self.pool)
        .await
    }

    /// A person's live personalized link for an event, if one exists —
    /// used to avoid minting duplicates on bulk add.
    pub async fn find_personal_link(
        &self,
        account_id: i64,
        event_id: i64,
        person_id: i64,
    ) -> sqlx::Result<Option<EventLink>> {
        sqlx::query_as!(
            EventLink,
            r#"SELECT id as "id!: i64", event_id as "event_id!: i64", person_id,
                      token_plain, label, tier, revoked_at, uses as "uses!: i64",
                      last_used_at, created_at
               FROM event_links
               WHERE account_id = ?1 AND event_id = ?2 AND person_id = ?3 AND revoked_at IS NULL"#,
            account_id,
            event_id,
            person_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn revoke_event_link(&self, account_id: i64, link_id: i64) -> sqlx::Result<u64> {
        let result = sqlx::query!(
            "UPDATE event_links SET revoked_at = datetime('now')
             WHERE account_id = ?1 AND id = ?2 AND revoked_at IS NULL",
            account_id,
            link_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::personal_token;

    #[test]
    fn personal_token_is_name_slug_plus_4char_suffix() {
        let token = personal_token("Maya Chen");
        let (slug, suffix) = token.rsplit_once('-').unwrap();
        assert_eq!(slug, "maya-chen");
        assert_eq!(suffix.len(), 4);
        assert!(suffix.bytes().all(|b| super::SUFFIX_ALPHABET.contains(&b)));

        // Two mints for the same name must differ (that's the whole
        // invalidation story).
        assert_ne!(personal_token("Maya Chen"), token);
    }

    #[test]
    fn personal_token_survives_hostile_names() {
        let token = personal_token("  Zoë O'Brien-Smith!! ");
        let (slug, _) = token.rsplit_once('-').unwrap();
        assert_eq!(slug, "zo-o-brien-smith");
        // All-symbol names still produce a usable token.
        assert!(personal_token("!!!").starts_with("guest-"));
    }
}
