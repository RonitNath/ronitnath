//! The longitudinal view: every person across every event, with how many
//! gatherings they've been part of. Admin-only.

use askama::Template;
use axum::Form;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::auth::extract::NavUser;
use crate::auth::{AccountScope, Role, csrf};
use crate::error::AppError;
use crate::state::AppState;
use crate::store::people::{Person, PersonHistory};
use crate::store::person_identity_links::ClaimStatus;
use crate::view::render;

#[derive(Template)]
#[template(path = "events/people.html")]
struct PeopleTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    people: Vec<PersonHistory>,
}

pub async fn page(
    State(state): State<AppState>,
    scope: AccountScope,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let people = state
        .store()
        .list_people_with_history(scope.account_id)
        .await?;
    render(PeopleTemplate {
        nav_active: "people",
        current_user: Some(NavUser {
            display_name: scope.display_name.clone(),
            csrf_token: scope.csrf_token.clone().unwrap_or_default(),
            is_guest: false,
        }),
        csrf_token: scope.csrf_token.unwrap_or_default(),
        people,
    })
}

#[derive(Deserialize)]
pub struct UpdatePersonForm {
    name: String,
    nickname: String,
    return_to: String,
    csrf_token: String,
}

pub async fn update(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(person_id): Path<i64>,
    Form(form): Form<UpdatePersonForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    let name = form.name.trim();
    if name.is_empty() {
        return Err(AppError::Invalid("name is required".into()));
    }
    let updated = state
        .store()
        .update_person(scope.account_id, person_id, name, form.nickname.trim())
        .await?;
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "person.updated",
            "person",
            Some(&person_id.to_string()),
            &serde_json::json!({ "name": name, "nickname": form.nickname.trim() }),
        )
        .await?;
    Ok(Redirect::to(safe_return_to(&form.return_to)).into_response())
}

#[derive(Template)]
#[template(path = "people/claim_status.html")]
struct ClaimStatusTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    person: Person,
    status: Option<ClaimStatus>,
}

pub async fn claim_status(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(person_id): Path<i64>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let person = state
        .store()
        .find_person(scope.account_id, person_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let status = state
        .store()
        .claim_status(scope.account_id, person_id)
        .await?;
    render(ClaimStatusTemplate {
        nav_active: "people",
        current_user: Some(NavUser {
            display_name: scope.display_name.clone(),
            csrf_token: scope.csrf_token.clone().unwrap_or_default(),
            is_guest: false,
        }),
        csrf_token: scope.csrf_token.unwrap_or_default(),
        person,
        status,
    })
}

#[derive(Deserialize)]
pub struct ForceUnlinkForm {
    csrf_token: String,
}

pub async fn force_unlink(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(person_id): Path<i64>,
    Form(form): Form<ForceUnlinkForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    let identity_id = state
        .store()
        .force_unlink_guest(scope.account_id, person_id)
        .await?
        .ok_or(AppError::NotFound)?;
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "guest.force_unlinked",
            "person",
            Some(&person_id.to_string()),
            &serde_json::json!({"identity_id": identity_id}),
        )
        .await?;
    Ok(Redirect::to(&format!("/people/{person_id}/claim-status")).into_response())
}

fn safe_return_to(value: &str) -> &str {
    if value.starts_with('/') && !value.starts_with("//") {
        value
    } else {
        "/people"
    }
}
