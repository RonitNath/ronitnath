//! Signup/login/logout business logic — the one place a verified factor
//! maps to a session, so audit calls aren't scattered. Phase 1 only has
//! `password`; once a federated factor exists, its "unknown + signup open
//! → create identity" / "unknown + authed → link" branches join here too.

use serde_json::json;
use time::macros::format_description;

use crate::auth::{password, session};
use crate::error::AppError;
use crate::state::AppState;

/// Request metadata that only matters for logging/session bookkeeping —
/// bundled so `signup`/`login` don't grow a positional parameter per field.
#[derive(Default)]
pub struct RequestContext<'a> {
    pub request_id: Option<&'a str>,
    pub ip: Option<&'a str>,
    pub user_agent: Option<&'a str>,
}

pub struct LoginOutcome {
    pub raw_token: String,
    pub ttl_secs: i64,
}

pub async fn signup(
    state: &AppState,
    display_name: &str,
    email: &str,
    password_plain: &str,
    ctx: RequestContext<'_>,
) -> Result<LoginOutcome, AppError> {
    let display_name = display_name.trim();
    let email = email.trim().to_lowercase();
    if display_name.is_empty() || email.is_empty() || password_plain.len() < 8 {
        return Err(AppError::Invalid(
            "name, email, and an 8+ character password are required".into(),
        ));
    }

    let hash =
        password::hash(password_plain).map_err(|e| anyhow::anyhow!("hashing password: {e}"))?;
    let (identity_id, account_id) = state
        .store()
        .signup_with_password(display_name, &email, &hash)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                AppError::Invalid("an account with that email already exists".into())
            }
            _ => AppError::from(e),
        })?;

    state
        .store()
        .audit(
            Some(identity_id),
            Some(account_id),
            ctx.request_id,
            "identity.signed_up",
            "identity",
            Some(&identity_id.to_string()),
            &json!({ "email": email }),
        )
        .await?;

    issue_session(state, identity_id, account_id, ctx).await
}

pub async fn login(
    state: &AppState,
    email: &str,
    password_plain: &str,
    ctx: RequestContext<'_>,
) -> Result<LoginOutcome, AppError> {
    let email = email.trim().to_lowercase();
    let factor = state
        .store()
        .find_factor_by_external("password", &email)
        .await?;

    // `password::verify` always runs — against a dummy hash when `factor`
    // is `None` — so "unknown email" and "wrong password" take the same
    // amount of time and can't be told apart by an attacker timing
    // responses.
    let verified = password::verify(
        password_plain,
        factor.as_ref().and_then(|f| f.secret_hash.as_deref()),
    );
    if !verified {
        state
            .store()
            .audit(
                None,
                None,
                ctx.request_id,
                "login.failed",
                "identity",
                None,
                &json!({ "email": email }),
            )
            .await?;
        return Err(AppError::InvalidCredentials(
            "incorrect email or password".into(),
        ));
    }
    let factor = factor.expect("verify() only returns true against a real hash");

    let membership = state
        .store()
        .find_primary_membership(factor.identity_id)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "identity {} has a password factor but no membership",
                factor.identity_id
            )
        })?;

    state.store().touch_factor_last_used(factor.id).await?;
    state
        .store()
        .audit(
            Some(factor.identity_id),
            Some(membership.account_id),
            ctx.request_id,
            "login.succeeded",
            "identity",
            Some(&factor.identity_id.to_string()),
            &json!({}),
        )
        .await?;

    issue_session(state, factor.identity_id, membership.account_id, ctx).await
}

/// Guest login uses recovery email only as a locator; the password factor keeps
/// its synthetic `guest:{person_id}` external id. Unknown/ambiguous emails still
/// execute the same dummy Argon2 verification as admin login.
pub async fn guest_login(
    state: &AppState,
    email: &str,
    password_plain: &str,
    ctx: RequestContext<'_>,
) -> Result<LoginOutcome, AppError> {
    let email = email.trim().to_lowercase();
    let factor = state.store().find_guest_password_by_email(&email).await?;
    let verified = password::verify(
        password_plain,
        factor.as_ref().and_then(|f| f.secret_hash.as_deref()),
    );
    if !verified {
        state
            .store()
            .audit(
                None,
                state.owner_account_id(),
                ctx.request_id,
                "guest.login.failed",
                "identity",
                None,
                &json!({ "identifier_kind": "recovery_email" }),
            )
            .await?;
        return Err(AppError::InvalidCredentials(
            "incorrect email or password".into(),
        ));
    }
    let factor = factor.expect("verify() only returns true against a real hash");
    let membership = state
        .store()
        .find_primary_membership(factor.identity_id)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("guest identity {} has no membership", factor.identity_id)
        })?;
    state.store().touch_factor_last_used(factor.id).await?;
    state
        .store()
        .audit(
            Some(factor.identity_id),
            state.owner_account_id(),
            ctx.request_id,
            "guest.login.succeeded",
            "identity",
            Some(&factor.identity_id.to_string()),
            &json!({}),
        )
        .await?;
    issue_session(state, factor.identity_id, membership.account_id, ctx).await
}

pub async fn logout(state: &AppState, session_id: i64, identity_id: i64) -> Result<(), AppError> {
    state
        .store()
        .revoke_session(session_id, identity_id)
        .await?;
    Ok(())
}

async fn issue_session(
    state: &AppState,
    identity_id: i64,
    account_id: i64,
    ctx: RequestContext<'_>,
) -> Result<LoginOutcome, AppError> {
    let ttl_secs = state.auth_config().session_ttl_secs;
    let raw_token = session::generate_token();
    let token_hash = session::hash_token(&raw_token);
    let csrf_token = session::generate_token();

    // sqlite's own `datetime('now')` format (space-separated, no
    // timezone suffix) — `find_session_context`'s `expires_at >
    // datetime('now')` is a plain string comparison, so this has to be
    // byte-for-byte the same shape or "not expired" would compare wrong.
    let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    let expires_at = (time::OffsetDateTime::now_utc() + time::Duration::seconds(ttl_secs))
        .format(&format)
        .map_err(|e| anyhow::anyhow!("formatting session expiry: {e}"))?;

    state
        .store()
        .create_session(
            identity_id,
            account_id,
            &token_hash,
            &csrf_token,
            &expires_at,
            ctx.user_agent,
            ctx.ip,
        )
        .await?;

    Ok(LoginOutcome {
        raw_token,
        ttl_secs,
    })
}
