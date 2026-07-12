//! Admin-only circles CRUD and membership forms.

use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::{
    auth::extract::NavUser,
    auth::{AccountScope, Role, csrf},
    error::AppError,
    state::AppState,
    store::{
        circles::{Circle, CircleMember},
        people::Person,
    },
    view::render,
};

fn nav(scope: &AccountScope) -> Option<NavUser> {
    Some(NavUser {
        display_name: scope.display_name.clone(),
        csrf_token: scope.csrf_token.clone().unwrap_or_default(),
    })
}

#[derive(Template)]
#[template(path = "circles/list.html")]
struct ListTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    circles: Vec<Circle>,
}

pub async fn list(
    State(state): State<AppState>,
    scope: AccountScope,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    render(ListTemplate {
        nav_active: "circles",
        current_user: nav(&scope),
        csrf_token: scope.csrf_token.clone().unwrap_or_default(),
        circles: state.store().list_circles(scope.account_id).await?,
    })
}

#[derive(Deserialize)]
pub struct NameForm {
    name: String,
    csrf_token: String,
}
pub async fn create(
    State(state): State<AppState>,
    scope: AccountScope,
    Form(form): Form<NameForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    let name = form.name.trim();
    if name.is_empty() {
        return Err(AppError::Invalid("circle name is required".into()));
    }
    let id = state.store().create_circle(scope.account_id, name).await?;
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "circle.created",
            "circle",
            Some(&id.to_string()),
            &serde_json::json!({"name": name}),
        )
        .await?;
    Ok(Redirect::to(&format!("/circles/{id}")).into_response())
}

#[derive(Template)]
#[template(path = "circles/detail.html")]
struct DetailTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    circle: Circle,
    members: Vec<CircleMember>,
    people: Vec<Person>,
}

pub async fn detail(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let circle = state
        .store()
        .find_circle(scope.account_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    render(DetailTemplate {
        nav_active: "circles",
        current_user: nav(&scope),
        csrf_token: scope.csrf_token.clone().unwrap_or_default(),
        members: state
            .store()
            .list_circle_members(scope.account_id, id)
            .await?,
        people: state.store().list_people(scope.account_id).await?,
        circle,
    })
}

pub async fn rename(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(id): Path<i64>,
    Form(form): Form<NameForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    if form.name.trim().is_empty() {
        return Err(AppError::Invalid("circle name is required".into()));
    }
    if state
        .store()
        .rename_circle(scope.account_id, id, form.name.trim())
        .await?
        == 0
    {
        return Err(AppError::NotFound);
    }
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "circle.renamed",
            "circle",
            Some(&id.to_string()),
            &serde_json::json!({"name": form.name.trim()}),
        )
        .await?;
    Ok(Redirect::to(&format!("/circles/{id}")).into_response())
}

#[derive(Deserialize)]
pub struct MemberForm {
    person_id: i64,
    csrf_token: String,
}
pub async fn add_member(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(id): Path<i64>,
    Form(form): Form<MemberForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    if state
        .store()
        .add_circle_member(scope.account_id, id, form.person_id)
        .await?
        == 0
    {
        return Err(AppError::NotFound);
    }
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "circle.member_added",
            "circle",
            Some(&id.to_string()),
            &serde_json::json!({"person_id": form.person_id}),
        )
        .await?;
    Ok(Redirect::to(&format!("/circles/{id}")).into_response())
}

#[derive(Deserialize)]
pub struct CsrfForm {
    csrf_token: String,
}
pub async fn remove_member(
    State(state): State<AppState>,
    scope: AccountScope,
    Path((id, person_id)): Path<(i64, i64)>,
    Form(form): Form<CsrfForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    if state
        .store()
        .remove_circle_member(scope.account_id, id, person_id)
        .await?
        == 0
    {
        return Err(AppError::NotFound);
    }
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "circle.member_removed",
            "circle",
            Some(&id.to_string()),
            &serde_json::json!({"person_id": person_id}),
        )
        .await?;
    Ok(Redirect::to(&format!("/circles/{id}")).into_response())
}

pub async fn delete(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(id): Path<i64>,
    Form(form): Form<CsrfForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    if state.store().delete_circle(scope.account_id, id).await? == 0 {
        return Err(AppError::NotFound);
    }
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "circle.deleted",
            "circle",
            Some(&id.to_string()),
            &serde_json::json!({}),
        )
        .await?;
    Ok(Redirect::to("/circles").into_response())
}
