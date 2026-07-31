//! Settings: factors (add/remove), api tokens (mint/revoke — a token is
//! just an `api_token` factor), and active sessions (revoke).

use askama::Template;
use axum::Form;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::auth::extract::NavUser;
use crate::auth::{AccountScope, Role, api_token, csrf, oidc};
use crate::error::AppError;
use crate::state::AppState;
use crate::store::factors::Factor;
use crate::store::sessions::SessionSummary;
use crate::view::render;

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    account_name: String,
    factors: Vec<Factor>,
    sessions: Vec<SessionSummary>,
    can_remove_factor: bool,
    minted_token: Option<String>,
    csrf_token: String,
    is_admin: bool,
    oidc_providers: Vec<oidc::OidcProviderButton>,
}

async fn render_page(
    state: &AppState,
    scope: &AccountScope,
    minted_token: Option<String>,
) -> Result<Response, AppError> {
    let factors = state.store().list_factors(scope.identity_id).await?;
    let sessions = state
        .store()
        .list_sessions(scope.identity_id, scope.session_id.unwrap_or(-1))
        .await?;
    let csrf_token = scope.csrf_token.clone().unwrap_or_default();

    render(SettingsTemplate {
        nav_active: "",
        current_user: Some(NavUser {
            display_name: scope.display_name.clone(),
            csrf_token: csrf_token.clone(),
            is_guest: false,
        }),
        account_name: scope.account_name.clone(),
        can_remove_factor: factors.len() > 1,
        factors,
        sessions,
        minted_token,
        csrf_token,
        is_admin: scope.role >= Role::Admin,
        oidc_providers: state.oidc().buttons(),
    })
}

pub async fn page(
    State(state): State<AppState>,
    scope: AccountScope,
) -> Result<Response, AppError> {
    render_page(&state, &scope, None).await
}

#[derive(Deserialize)]
pub struct CsrfForm {
    csrf_token: String,
}

pub async fn mint_token(
    State(state): State<AppState>,
    scope: AccountScope,
    Form(form): Form<CsrfForm>,
) -> Result<Response, AppError> {
    csrf::verify(&scope, &form.csrf_token)?;
    let raw = api_token::mint(state.store(), scope.identity_id).await?;
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "factor.linked",
            "factor",
            None,
            &serde_json::json!({ "kind": "api_token" }),
        )
        .await?;
    render_page(&state, &scope, Some(raw)).await
}

pub async fn remove_factor(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(factor_id): Path<i64>,
    Form(form): Form<CsrfForm>,
) -> Result<Response, AppError> {
    csrf::verify(&scope, &form.csrf_token)?;
    if state.store().count_factors(scope.identity_id).await? <= 1 {
        return Err(AppError::Invalid(
            "can't remove your last login method — link another factor first".into(),
        ));
    }
    state
        .store()
        .delete_factor(factor_id, scope.identity_id)
        .await?;
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "factor.removed",
            "factor",
            Some(&factor_id.to_string()),
            &serde_json::json!({}),
        )
        .await?;
    Ok(Redirect::to("/settings").into_response())
}

pub async fn revoke_session(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(session_id): Path<i64>,
    Form(form): Form<CsrfForm>,
) -> Result<Response, AppError> {
    csrf::verify(&scope, &form.csrf_token)?;
    state
        .store()
        .revoke_session(session_id, scope.identity_id)
        .await?;
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "session.revoked",
            "session",
            Some(&session_id.to_string()),
            &serde_json::json!({}),
        )
        .await?;
    Ok(Redirect::to("/settings").into_response())
}
