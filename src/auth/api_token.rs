//! The `api_token` factor: bearer tokens for agent/service identities (and
//! humans scripting against the JSON API). Not a session/cookie flow — a
//! `Bearer` header checked directly by [`crate::auth::extract`]. Token
//! auth is CSRF-immune (a cross-site request can't read or set an
//! `Authorization` header), so it skips the CSRF check entirely.

use crate::auth::session;
use crate::store::Store;

/// What verifying a bearer token resolves to — deliberately the same
/// shape [`crate::auth::extract::AccountScope`] needs, so both auth paths
/// converge before reaching a handler.
pub struct VerifiedToken {
    pub identity_id: i64,
    pub display_name: String,
    pub account_id: i64,
    pub account_name: String,
    pub role: String,
}

/// Mints a new token for `identity_id`, returning the raw value — shown to
/// the caller exactly once (Settings page), never retrievable again since
/// only its hash is stored.
pub async fn mint(store: &Store, identity_id: i64) -> sqlx::Result<String> {
    let raw = session::generate_token();
    let hash = session::hash_token(&raw);
    store
        .create_factor(identity_id, "api_token", None, Some(&hash))
        .await?;
    Ok(raw)
}

pub async fn verify(store: &Store, raw_token: &str) -> sqlx::Result<Option<VerifiedToken>> {
    let hash = session::hash_token(raw_token);
    let Some(factor) = store.find_factor_by_secret_hash(&hash).await? else {
        return Ok(None);
    };
    store.touch_factor_last_used(factor.id).await?;

    let Some(identity) = store.find_identity(factor.identity_id).await? else {
        return Ok(None);
    };
    let Some(membership) = store.find_primary_membership(factor.identity_id).await? else {
        return Ok(None);
    };

    Ok(Some(VerifiedToken {
        identity_id: identity.id,
        display_name: identity.display_name,
        account_id: membership.account_id,
        account_name: membership.account_name,
        role: membership.role,
    }))
}
