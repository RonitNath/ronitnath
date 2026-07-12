//! Per-event audience policy editor. Forms persist inputs; level math is never duplicated here.

use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};
use std::collections::{HashMap, HashSet};

use crate::{
    access::level::Level,
    auth::extract::NavUser,
    auth::{AccountScope, Role, csrf},
    error::AppError,
    state::AppState,
    store::{
        audience::{AudiencePolicyRow, AudienceUpdate, CircleGrantRow, PersonOverrideRow},
        circles::Circle,
        events::Event,
        people::Person,
    },
    view::render,
};

#[derive(Template)]
#[template(path = "events/audience.html")]
struct AudienceTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    event: Event,
    policy: AudiencePolicyRow,
    circles: Vec<Circle>,
    grants: Vec<CircleGrantRow>,
    people: Vec<Person>,
    overrides: Vec<PersonOverrideRow>,
}

impl AudienceTemplate {
    fn circle_level(&self, id: &i64) -> &str {
        self.grants
            .iter()
            .find(|g| g.circle_id == *id)
            .map_or("none", |g| g.level.as_str())
    }
    fn person_override(&self, id: &i64) -> String {
        self.overrides
            .iter()
            .find(|o| o.person_id == *id)
            .map_or_else(
                || "none".into(),
                |o| {
                    if o.override_kind == "exclude" {
                        "exclude".into()
                    } else {
                        format!("include:{}", o.level.as_deref().unwrap_or("hidden"))
                    }
                },
            )
    }
}

pub async fn page(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(event_id): Path<i64>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let store = state.store();
    let event = store
        .find_event(scope.account_id, event_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let policy = store
        .find_audience_policy(scope.account_id, "event", event_id)
        .await?
        .ok_or(AppError::NotFound)?;
    render(AudienceTemplate {
        nav_active: "events",
        current_user: Some(NavUser {
            display_name: scope.display_name.clone(),
            csrf_token: scope.csrf_token.clone().unwrap_or_default(),
            is_guest: false,
        }),
        csrf_token: scope.csrf_token.clone().unwrap_or_default(),
        grants: store
            .list_audience_grants(scope.account_id, policy.id)
            .await?,
        overrides: store
            .list_audience_overrides(scope.account_id, policy.id)
            .await?,
        circles: store.list_circles(scope.account_id).await?,
        people: store.list_people(scope.account_id).await?,
        event,
        policy,
    })
}

pub(crate) fn parse_update(
    form: &HashMap<String, String>,
    circles: &[Circle],
    people: &[Person],
) -> Result<AudienceUpdate, AppError> {
    let known_circles = circles.iter().map(|row| row.id).collect::<HashSet<_>>();
    let known_people = people.iter().map(|row| row.id).collect::<HashSet<_>>();
    for key in form.keys() {
        if let Some(id) = key.strip_prefix("circle_") {
            let id = id
                .parse::<i64>()
                .map_err(|_| AppError::Invalid("invalid circle audience field".into()))?;
            if !known_circles.contains(&id) {
                return Err(AppError::Invalid("unknown circle audience field".into()));
            }
        } else if let Some(id) = key.strip_prefix("person_") {
            let id = id
                .parse::<i64>()
                .map_err(|_| AppError::Invalid("invalid person audience field".into()))?;
            if !known_people.contains(&id) {
                return Err(AppError::Invalid("unknown person audience field".into()));
            }
        }
    }
    let public = form
        .get("public_level")
        .ok_or_else(|| AppError::Invalid("public level is required".into()))?;
    public
        .parse::<Level>()
        .map_err(|e| AppError::Invalid(e.into()))?;
    let circles = circles
        .iter()
        .map(|circle| {
            let value = form
                .get(&format!("circle_{}", circle.id))
                .map(String::as_str)
                .unwrap_or("none");
            let level = if value == "none" {
                None
            } else {
                value
                    .parse::<Level>()
                    .map_err(|e| AppError::Invalid(e.into()))?;
                Some(value.to_owned())
            };
            Ok((circle.id, level))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let people = people
        .iter()
        .map(|person| {
            let value = form
                .get(&format!("person_{}", person.id))
                .map(String::as_str)
                .unwrap_or("none");
            let (kind, level) = if value == "none" {
                (None, None)
            } else if value == "exclude" {
                (Some("exclude".to_owned()), None)
            } else if let Some(level) = value.strip_prefix("include:") {
                level
                    .parse::<Level>()
                    .map_err(|e| AppError::Invalid(e.into()))?;
                (Some("include".to_owned()), Some(level.to_owned()))
            } else {
                return Err(AppError::Invalid("invalid person override".into()));
            };
            Ok((person.id, kind, level))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(AudienceUpdate {
        public_level: public.to_owned(),
        circles,
        people,
    })
}

pub async fn save(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(event_id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(
        &scope,
        form.get("csrf_token").map(String::as_str).unwrap_or(""),
    )?;
    let store = state.store();
    let policy = store
        .find_audience_policy(scope.account_id, "event", event_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let circles = store.list_circles(scope.account_id).await?;
    let people = store.list_people(scope.account_id).await?;
    let update = parse_update(&form, &circles, &people)?;
    store
        .apply_audience_update(
            scope.account_id,
            policy.id,
            scope.identity_id,
            "event",
            event_id,
            &update,
        )
        .await?;
    Ok(Redirect::to(&format!("/events/{event_id}/audience")).into_response())
}
