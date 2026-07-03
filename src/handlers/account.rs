//! Account settings — the role-gating exemplar. Every handler here calls
//! `scope.require(Role::Admin)` before doing anything, which is the
//! pattern to copy for any other admin-only route.

use askama::Template;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;

use crate::auth::extract::NavUser;
use crate::auth::{AccountScope, Role, csrf};
use crate::error::AppError;
use crate::state::AppState;
use crate::store::audit::AuditEntry;
use crate::view::render;

#[derive(Template)]
#[template(path = "account.html")]
struct AccountTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    account_name: String,
    csrf_token: String,
}

pub async fn page(scope: AccountScope) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    render(AccountTemplate {
        nav_active: "",
        current_user: Some(NavUser {
            display_name: scope.display_name,
            csrf_token: scope.csrf_token.clone().unwrap_or_default(),
        }),
        account_name: scope.account_name,
        csrf_token: scope.csrf_token.unwrap_or_default(),
    })
}

#[derive(Deserialize)]
pub struct RenameForm {
    name: String,
    csrf_token: String,
}

pub async fn rename(
    State(state): State<AppState>,
    scope: AccountScope,
    Form(form): Form<RenameForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    if form.name.trim().is_empty() {
        return Err(AppError::Invalid("account name must not be empty".into()));
    }

    state.store().rename_account(scope.account_id, form.name.trim()).await?;
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "account.renamed",
            "account",
            Some(&scope.account_id.to_string()),
            &serde_json::json!({ "name": form.name.trim() }),
        )
        .await?;
    Ok(Redirect::to("/account").into_response())
}

#[derive(Template)]
#[template(path = "account_audit.html")]
struct AuditTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    entries: Vec<AuditEntry>,
}

pub async fn audit(State(state): State<AppState>, scope: AccountScope) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let entries = state.store().list_audit_log(scope.account_id, 100).await?;
    render(AuditTemplate {
        nav_active: "",
        current_user: Some(NavUser {
            display_name: scope.display_name,
            csrf_token: scope.csrf_token.unwrap_or_default(),
        }),
        entries,
    })
}
