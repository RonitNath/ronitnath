//! Admin CRUD, audience editing, and per-person feed lifecycle.

use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    access::level::Level,
    auth::{
        AccountScope, Role, csrf,
        extract::NavUser,
        session::{generate_token, hash_token},
    },
    error::AppError,
    state::AppState,
    store::{
        audience::{AudiencePolicyRow, CircleGrantRow, PersonOverrideRow},
        calendar_entries::{CalendarEntry, CalendarEntryFields},
        circles::Circle,
        people::Person,
    },
    view::render,
};

fn nav_user(scope: &AccountScope) -> Option<NavUser> {
    Some(NavUser {
        display_name: scope.display_name.clone(),
        csrf_token: scope.csrf_token.clone().unwrap_or_default(),
        is_guest: false,
    })
}

#[derive(Template)]
#[template(path = "calendar/admin.html")]
struct AdminTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    entries: Vec<CalendarEntry>,
}

pub async fn list(
    State(state): State<AppState>,
    scope: AccountScope,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let entries = state
        .store()
        .list_calendar_entries(scope.account_id)
        .await?;
    render(AdminTemplate {
        nav_active: "calendar",
        current_user: nav_user(&scope),
        csrf_token: scope.csrf_token.unwrap_or_default(),
        entries,
    })
}

#[derive(Deserialize)]
pub struct EntryForm {
    title: String,
    location: String,
    starts_at: String,
    ends_at: String,
    timezone: String,
    notes: String,
    csrf_token: String,
    action: Option<String>,
}
fn fields(form: &EntryForm) -> Result<CalendarEntryFields<'_>, AppError> {
    if form.title.trim().is_empty() || form.starts_at.trim().is_empty() {
        return Err(AppError::Invalid(
            "title and start time are required".into(),
        ));
    }
    Ok(CalendarEntryFields {
        title: form.title.trim(),
        location: form.location.trim(),
        starts_at: form.starts_at.trim(),
        ends_at: (!form.ends_at.trim().is_empty()).then_some(form.ends_at.trim()),
        timezone: if form.timezone.trim().is_empty() {
            "America/Los_Angeles"
        } else {
            form.timezone.trim()
        },
        notes: form.notes.trim(),
    })
}
pub async fn create(
    State(state): State<AppState>,
    scope: AccountScope,
    Form(form): Form<EntryForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    let entry = state
        .store()
        .create_calendar_entry(scope.account_id, &fields(&form)?)
        .await?;
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "calendar_entry.created",
            "calendar_entry",
            Some(&entry.id.to_string()),
            &serde_json::json!({}),
        )
        .await?;
    Ok(Redirect::to(&format!("/calendar/entries/{}/audience", entry.id)).into_response())
}
pub async fn update(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(id): Path<i64>,
    Form(form): Form<EntryForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    let action = form.action.as_deref().unwrap_or("update");
    let affected = if action == "delete" {
        state
            .store()
            .delete_calendar_entry(scope.account_id, id)
            .await?
    } else {
        state
            .store()
            .update_calendar_entry(scope.account_id, id, &fields(&form)?)
            .await?
    };
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    let audit_action = if action == "delete" {
        "calendar_entry.deleted"
    } else {
        "calendar_entry.updated"
    };
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            audit_action,
            "calendar_entry",
            Some(&id.to_string()),
            &serde_json::json!({}),
        )
        .await?;
    Ok(Redirect::to("/calendar").into_response())
}

#[derive(Template)]
#[template(path = "calendar/audience.html")]
struct AudienceTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    entry: CalendarEntry,
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
pub async fn audience_page(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    let store = state.store();
    let entry = store
        .find_calendar_entry(scope.account_id, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let policy = store
        .find_audience_policy(scope.account_id, "calendar_entry", id)
        .await?
        .ok_or(AppError::NotFound)?;
    render(AudienceTemplate {
        nav_active: "calendar",
        current_user: nav_user(&scope),
        csrf_token: scope.csrf_token.clone().unwrap_or_default(),
        grants: store
            .list_audience_grants(scope.account_id, policy.id)
            .await?,
        overrides: store
            .list_audience_overrides(scope.account_id, policy.id)
            .await?,
        circles: store.list_circles(scope.account_id).await?,
        people: store.list_people(scope.account_id).await?,
        entry,
        policy,
    })
}
pub async fn audience_save(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(
        &scope,
        form.get("csrf_token").map(String::as_str).unwrap_or(""),
    )?;
    let public = form
        .get("public_level")
        .ok_or_else(|| AppError::Invalid("public level is required".into()))?;
    public
        .parse::<Level>()
        .map_err(|e| AppError::Invalid(e.into()))?;
    let store = state.store();
    let policy = store
        .find_audience_policy(scope.account_id, "calendar_entry", id)
        .await?
        .ok_or(AppError::NotFound)?;
    store
        .set_public_level(scope.account_id, policy.id, public)
        .await?;
    for circle in store.list_circles(scope.account_id).await? {
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
            Some(value)
        };
        store
            .set_circle_grant(scope.account_id, policy.id, circle.id, level)
            .await?;
    }
    for person in store.list_people(scope.account_id).await? {
        let value = form
            .get(&format!("person_{}", person.id))
            .map(String::as_str)
            .unwrap_or("none");
        let (kind, level) = if value == "none" {
            (None, None)
        } else if value == "exclude" {
            (Some("exclude"), None)
        } else if let Some(level) = value.strip_prefix("include:") {
            level
                .parse::<Level>()
                .map_err(|e| AppError::Invalid(e.into()))?;
            (Some("include"), Some(level))
        } else {
            return Err(AppError::Invalid("invalid person override".into()));
        };
        store
            .set_person_override(scope.account_id, policy.id, person.id, kind, level)
            .await?;
    }
    store
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            "audience.updated",
            "calendar_entry",
            Some(&id.to_string()),
            &serde_json::json!({"public_level":public}),
        )
        .await?;
    Ok(Redirect::to(&format!("/calendar/entries/{id}/audience")).into_response())
}

#[derive(Template)]
#[template(path = "people/calendar_feed.html")]
struct FeedTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    csrf_token: String,
    person: Person,
    url: Option<String>,
    last_used_at: Option<String>,
}
pub async fn feed_page(
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
    let feed = state
        .store()
        .find_calendar_feed_for_person(scope.account_id, person_id)
        .await?;
    let (url, last_used_at) = match feed.filter(|f| f.revoked_at.is_none()) {
        Some(f) => (
            Some(format!(
                "{}/calendar/{}.ics",
                state.public_url(),
                f.token_plain
            )),
            f.last_used_at,
        ),
        None => (None, None),
    };
    render(FeedTemplate {
        nav_active: "people",
        current_user: nav_user(&scope),
        csrf_token: scope.csrf_token.unwrap_or_default(),
        person,
        url,
        last_used_at,
    })
}
#[derive(Deserialize)]
pub struct FeedForm {
    csrf_token: String,
    action: String,
}
pub async fn feed_action(
    State(state): State<AppState>,
    scope: AccountScope,
    Path(person_id): Path<i64>,
    Form(form): Form<FeedForm>,
) -> Result<Response, AppError> {
    scope.require(Role::Admin)?;
    csrf::verify(&scope, &form.csrf_token)?;
    state
        .store()
        .find_person(scope.account_id, person_id)
        .await?
        .ok_or(AppError::NotFound)?;
    match form.action.as_str() {
        "mint" => {
            let raw = generate_token();
            state
                .store()
                .mint_calendar_feed(scope.account_id, person_id, &hash_token(&raw), &raw)
                .await?;
        }
        "revoke" => {
            if state
                .store()
                .revoke_calendar_feed(scope.account_id, person_id)
                .await?
                == 0
            {
                return Err(AppError::NotFound);
            }
        }
        _ => return Err(AppError::Invalid("action must be mint or revoke".into())),
    };
    state
        .store()
        .audit(
            Some(scope.identity_id),
            Some(scope.account_id),
            None,
            &format!("calendar_feed.{}", form.action),
            "person",
            Some(&person_id.to_string()),
            &serde_json::json!({}),
        )
        .await?;
    Ok(Redirect::to(&format!("/people/{person_id}/calendar-feed")).into_response())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_util::{post_form_with_cookie, signup, test_app};

    #[tokio::test]
    async fn calendar_admin_mutations_require_csrf_and_persist_lifecycle() {
        let (app, store) = test_app().await;
        let owner = signup(
            &app,
            "Calendar Owner",
            "calendar-admin@example.com",
            "password123",
        )
        .await;
        let account_id = store.find_primary_account().await.unwrap().unwrap();
        let form = "title=Admin+Entry&location=Admin+Place&starts_at=2099-07-20+10%3A00&ends_at=&timezone=America%2FLos_Angeles&notes=Admin+Notes&csrf_token=";
        assert_eq!(
            post_form_with_cookie(&app, "/calendar/entries", form, Some(&owner.cookie))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let form = format!("{form}{}", owner.csrf_token);
        assert_eq!(
            post_form_with_cookie(&app, "/calendar/entries", &form, Some(&owner.cookie))
                .await
                .0,
            StatusCode::SEE_OTHER
        );
        let entry = store
            .list_calendar_entries(account_id)
            .await
            .unwrap()
            .pop()
            .unwrap();
        let audience = format!("csrf_token={}&public_level=summary", owner.csrf_token);
        assert_eq!(
            post_form_with_cookie(
                &app,
                &format!("/calendar/entries/{}/audience", entry.id),
                &audience,
                Some(&owner.cookie),
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            store
                .find_audience_policy(account_id, "calendar_entry", entry.id)
                .await
                .unwrap()
                .unwrap()
                .public_level,
            "summary"
        );

        let person = store
            .create_person(account_id, "Feed Recipient", "")
            .await
            .unwrap();
        assert_eq!(
            post_form_with_cookie(
                &app,
                &format!("/people/{}/calendar-feed", person.id),
                "action=mint&csrf_token=wrong",
                Some(&owner.cookie),
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        let mint = format!("action=mint&csrf_token={}", owner.csrf_token);
        assert_eq!(
            post_form_with_cookie(
                &app,
                &format!("/people/{}/calendar-feed", person.id),
                &mint,
                Some(&owner.cookie),
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert!(
            store
                .find_calendar_feed_for_person(account_id, person.id)
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_none()
        );
        let revoke = format!("action=revoke&csrf_token={}", owner.csrf_token);
        assert_eq!(
            post_form_with_cookie(
                &app,
                &format!("/people/{}/calendar-feed", person.id),
                &revoke,
                Some(&owner.cookie),
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
        assert!(
            store
                .find_calendar_feed_for_person(account_id, person.id)
                .await
                .unwrap()
                .unwrap()
                .revoked_at
                .is_some()
        );
    }
}
