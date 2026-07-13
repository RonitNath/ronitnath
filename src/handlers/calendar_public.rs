//! Public month browsing and revocable person-bound calendar feeds.
//! Both are read-time unions; no event is mirrored into `calendar_entries`.

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use chrono::{Datelike, NaiveDate};

const MIN_CALENDAR_YEAR: i32 = 1970;
const MAX_CALENDAR_YEAR: i32 = 2100;
use serde::Deserialize;

use crate::{
    access::level::Level,
    auth::{
        extract::{NavContext, NavUser},
        session::hash_token,
        viewer::Viewer,
    },
    error::AppError,
    state::AppState,
    store::{calendar_entries::CalendarEntryView, events::EventView},
    view::render,
};

#[derive(Debug, Clone)]
pub struct CalendarItem {
    pub source: &'static str,
    pub source_id: i64,
    pub title: String,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub timezone: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub level: &'static str,
}

#[derive(Debug)]
struct CalendarDay {
    number: u32,
    date: String,
    items: Vec<CalendarItem>,
}

#[derive(Template)]
#[template(path = "calendar/month.html")]
struct CalendarTemplate {
    nav_active: &'static str,
    current_user: Option<NavUser>,
    month_label: String,
    previous_month: String,
    next_month: String,
    leading_blanks: usize,
    days: Vec<CalendarDay>,
    agenda: Vec<CalendarItem>,
}

#[derive(Deserialize)]
pub struct MonthQuery {
    month: Option<String>,
}

pub async fn page(
    State(state): State<AppState>,
    viewer: Viewer,
    NavContext(current_user): NavContext,
    Query(query): Query<MonthQuery>,
) -> Result<Response, AppError> {
    let first = parse_month(query.month.as_deref())?;
    let next = next_month(first)?;
    let account_id = match state.owner_account_id() {
        Some(id) => id,
        None => match state.store().find_primary_account().await? {
            Some(id) => id,
            None => return calendar_response(first, current_user, Vec::new()),
        },
    };
    let items = union_for_range(
        &state,
        account_id,
        &viewer,
        &format!("{} 00:00:00", first.format("%Y-%m-%d")),
        &format!("{} 00:00:00", next.format("%Y-%m-%d")),
    )
    .await?;
    calendar_response(first, current_user, items)
}

fn calendar_response(
    first: NaiveDate,
    current_user: Option<NavUser>,
    items: Vec<CalendarItem>,
) -> Result<Response, AppError> {
    let next = next_month(first)?;
    let (previous_year, previous_month) = if first.month() == 1 {
        (first.year().checked_sub(1), 12)
    } else {
        (Some(first.year()), first.month() - 1)
    };
    let previous = previous_year
        .and_then(|year| NaiveDate::from_ymd_opt(year, previous_month, 1))
        .ok_or_else(|| AppError::Invalid("month is outside the supported range".into()))?;
    let mut days = Vec::new();
    let mut cursor = first;
    while cursor < next {
        let prefix = cursor.format("%Y-%m-%d").to_string();
        days.push(CalendarDay {
            number: cursor.day(),
            date: prefix.clone(),
            items: items
                .iter()
                .filter(|item| item.starts_at.starts_with(&prefix))
                .cloned()
                .collect(),
        });
        cursor = cursor
            .succ_opt()
            .ok_or_else(|| AppError::Invalid("month is outside the supported range".into()))?;
    }
    let mut response = render(CalendarTemplate {
        nav_active: "calendar",
        current_user,
        month_label: first.format("%B %Y").to_string(),
        previous_month: previous.format("%Y-%m").to_string(),
        next_month: next.format("%Y-%m").to_string(),
        leading_blanks: first.weekday().num_days_from_sunday() as usize,
        days,
        agenda: items,
    })?;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

fn parse_month(value: Option<&str>) -> Result<NaiveDate, AppError> {
    let parsed = match value {
        Some(value) => NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d")
            .map_err(|_| AppError::Invalid("month must be YYYY-MM".into()))?,
        None => {
            let today = chrono::Local::now().date_naive();
            NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
                .ok_or_else(|| AppError::Invalid("current month is invalid".into()))?
        }
    };
    if !(MIN_CALENDAR_YEAR..=MAX_CALENDAR_YEAR).contains(&parsed.year()) {
        return Err(AppError::Invalid(format!(
            "month year must be between {MIN_CALENDAR_YEAR} and {MAX_CALENDAR_YEAR}"
        )));
    }
    Ok(parsed)
}
fn next_month(first: NaiveDate) -> Result<NaiveDate, AppError> {
    let (year, month) = if first.month() == 12 {
        (first.year().checked_add(1), 1)
    } else {
        (Some(first.year()), first.month() + 1)
    };
    year.and_then(|year| NaiveDate::from_ymd_opt(year, month, 1))
        .ok_or_else(|| AppError::Invalid("month is outside the supported range".into()))
}

async fn union_for_range(
    state: &AppState,
    account_id: i64,
    viewer: &Viewer,
    start: &str,
    end: &str,
) -> Result<Vec<CalendarItem>, AppError> {
    let mut items = Vec::new();
    for event in state.store().list_events(account_id).await? {
        if event.status == "draft"
            || event.starts_at.as_str() < start
            || event.starts_at.as_str() >= end
        {
            continue;
        }
        let Some(inputs) = state
            .store()
            .audience_inputs_for_event(account_id, event.id, viewer.person_id())
            .await?
        else {
            continue;
        };
        let level = inputs.level_for(viewer)?;
        let Some(view) = event.view_for(level) else {
            continue;
        };
        items.push(event_item(event.id, view, level));
    }
    for entry in state
        .store()
        .list_calendar_entries_range(account_id, start, end)
        .await?
    {
        let Some(inputs) = state
            .store()
            .audience_inputs_for_calendar_entry(account_id, entry.id, viewer.person_id())
            .await?
        else {
            continue;
        };
        let level = inputs.level_for(viewer)?;
        let Some(view) = entry.view_for(level) else {
            continue;
        };
        items.push(entry_item(entry.id, view, level));
    }
    // Each source row is appended exactly once; coincident event/entry times
    // remain two intentional items rather than multiplying through SQL joins.
    items.sort_by(|a, b| {
        a.starts_at
            .cmp(&b.starts_at)
            .then(a.source.cmp(b.source))
            .then(a.source_id.cmp(&b.source_id))
    });
    Ok(items)
}

fn event_item(id: i64, view: EventView, level: Level) -> CalendarItem {
    match view {
        EventView::Busy(v) => CalendarItem {
            source: "event",
            source_id: id,
            title: "Busy".into(),
            starts_at: v.starts_at,
            ends_at: v.ends_at,
            timezone: v.timezone,
            location: None,
            notes: None,
            level: "busy",
        },
        EventView::Event(v) => {
            let summary = (!v.summary.is_empty()).then_some(v.summary);
            let private_details = v.private_details.filter(|value| !value.is_empty());
            CalendarItem {
                source: "event",
                source_id: id,
                title: v.title,
                starts_at: v.starts_at,
                ends_at: v.ends_at,
                timezone: v.timezone,
                location: v.address,
                notes: private_details.or(summary),
                level: level.as_str(),
            }
        }
    }
}
fn entry_item(id: i64, view: CalendarEntryView, level: Level) -> CalendarItem {
    match view {
        CalendarEntryView::Busy(v) => CalendarItem {
            source: "entry",
            source_id: id,
            title: "Busy".into(),
            starts_at: v.starts_at,
            ends_at: v.ends_at,
            timezone: v.timezone,
            location: None,
            notes: None,
            level: "busy",
        },
        CalendarEntryView::Entry(v) => CalendarItem {
            source: "entry",
            source_id: id,
            title: v.title,
            starts_at: v.starts_at,
            ends_at: v.ends_at,
            timezone: v.timezone,
            location: v.location,
            notes: v.notes,
            level: level.as_str(),
        },
    }
}

pub async fn feed(
    State(state): State<AppState>,
    Path(feed_path): Path<String>,
) -> Result<Response, AppError> {
    let token = feed_path.strip_suffix(".ics").ok_or(AppError::NotFound)?;
    let feed = state
        .store()
        .resolve_calendar_feed(&hash_token(token))
        .await?
        .ok_or(AppError::NotFound)?;
    if state.store().touch_calendar_feed(feed.id).await? == 0 {
        return Err(AppError::NotFound);
    }
    let viewer = Viewer::FeedHolder {
        person_id: feed.person_id,
    };
    let mut items = Vec::new();
    for event in state.store().list_events(feed.account_id).await? {
        if event.status == "draft" {
            continue;
        }
        let Some(inputs) = state
            .store()
            .audience_inputs_for_event(feed.account_id, event.id, Some(feed.person_id))
            .await?
        else {
            continue;
        };
        let level = inputs.level_for(&viewer)?;
        if let Some(view) = event.view_for(level) {
            items.push(event_item(event.id, view, level));
        }
    }
    for entry in state.store().list_calendar_entries(feed.account_id).await? {
        let Some(inputs) = state
            .store()
            .audience_inputs_for_calendar_entry(feed.account_id, entry.id, Some(feed.person_id))
            .await?
        else {
            continue;
        };
        let level = inputs.level_for(&viewer)?;
        if let Some(view) = entry.view_for(level) {
            items.push(entry_item(entry.id, view, level));
        }
    }
    items.sort_by(|a, b| a.starts_at.cmp(&b.starts_at));
    let mut body =
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//ronitnath//calendar//EN\r\n".to_string();
    for item in items {
        let end = item.ends_at.as_deref().unwrap_or(&item.starts_at);
        let (location, description) = if item.level == "full" {
            (item.location.as_deref(), item.notes.as_deref())
        } else {
            (None, None)
        };
        body.push_str(&super::event_public::format_vevent(
            &format!("{}-{}", item.source, item.source_id),
            &item.title,
            &item.starts_at,
            end,
            &item.timezone,
            location,
            description,
        ));
    }
    body.push_str("END:VCALENDAR\r\n");
    Ok((
        [
            (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        body,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;
    use crate::{
        app::{build_site_router, test_auth_config},
        auth::session::{generate_token, hash_token},
        config::Config,
        state::AppState,
        store::{Store, calendar_entries::CalendarEntryFields, events::EventFields},
        test_util::{get, get_with_cookie, seed_session},
    };

    async fn site(store: &Store, owner_account_id: i64) -> axum::Router {
        let config = Config::for_tests();
        let state = AppState::new(store.clone(), test_auth_config(&config))
            .with_owner_account_id(Some(owner_account_id));
        build_site_router(state, &config)
    }

    async fn entry(store: &Store, account_id: i64, title: &str, starts_at: &str) -> i64 {
        store
            .create_calendar_entry(
                account_id,
                &CalendarEntryFields {
                    title,
                    location: &format!("{title} Location"),
                    starts_at,
                    ends_at: Some("2099-07-12 11:00"),
                    timezone: "America/Los_Angeles",
                    notes: &format!("{title} Notes"),
                },
            )
            .await
            .unwrap()
            .id
    }

    async fn set_public(store: &Store, account_id: i64, subject: &str, id: i64, level: &str) {
        let policy = store
            .find_audience_policy(account_id, subject, id)
            .await
            .unwrap()
            .unwrap();
        store
            .set_public_level(account_id, policy.id, level)
            .await
            .unwrap();
    }

    async fn guest_cookie(store: &Store, account_id: i64, person_id: i64) -> String {
        let raw = generate_token();
        store
            .claim_guest(
                account_id,
                person_id,
                "Calendar Guest",
                None,
                "test-hash",
                &hash_token(&raw),
                "test-csrf",
                "9999-01-01 00:00:00",
                None,
                None,
            )
            .await
            .unwrap();
        format!("session={raw}")
    }

    fn text(body: axum::body::Bytes) -> String {
        String::from_utf8(body.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn chrono_extreme_month_boundaries_are_clean_client_errors() {
        let store = Store::connect_in_memory().await;
        let (_, account_id) = store
            .signup_with_password("Owner", "month-boundary@example.com", "hash")
            .await
            .unwrap();
        let app = site(&store, account_id).await;
        for path in [
            "/calendar?month=%2B262142-12",
            "/calendar?month=-262143-01",
            "/calendar?month=2101-01",
            "/calendar?month=not-a-month",
        ] {
            let status = get(&app, path).await.0;
            assert!(status.is_client_error(), "{path} returned {status}");
        }
    }

    #[tokio::test]
    async fn month_leak_matrix_and_read_time_union_do_not_duplicate_sources() {
        let store = Store::connect_in_memory().await;
        let (owner_identity_id, account_id) = store
            .signup_with_password("Owner", "calendar-owner@example.com", "hash")
            .await
            .unwrap();

        let busy_id = entry(&store, account_id, "BUSY SECRET", "2099-07-08 10:00").await;
        set_public(&store, account_id, "calendar_entry", busy_id, "busy").await;

        let circle_id = store.create_circle(account_id, "Friends").await.unwrap();
        let circle_person = store
            .create_person(account_id, "Circle Guest", "")
            .await
            .unwrap();
        store
            .add_circle_member(account_id, circle_id, circle_person.id)
            .await
            .unwrap();
        let circle_entry = entry(&store, account_id, "Circle Summary", "2099-07-09 10:00").await;
        let circle_policy = store
            .find_audience_policy(account_id, "calendar_entry", circle_entry)
            .await
            .unwrap()
            .unwrap();
        store
            .set_circle_grant(account_id, circle_policy.id, circle_id, Some("summary"))
            .await
            .unwrap();

        let excluded_person = store
            .create_person(account_id, "Excluded Guest", "")
            .await
            .unwrap();
        let excluded_entry = entry(&store, account_id, "Excluded Public", "2099-07-10 10:00").await;
        set_public(
            &store,
            account_id,
            "calendar_entry",
            excluded_entry,
            "summary",
        )
        .await;
        let excluded_policy = store
            .find_audience_policy(account_id, "calendar_entry", excluded_entry)
            .await
            .unwrap()
            .unwrap();
        store
            .set_person_override(
                account_id,
                excluded_policy.id,
                excluded_person.id,
                Some("exclude"),
                None,
            )
            .await
            .unwrap();

        entry(&store, account_id, "Owner Secret", "2099-07-11 10:00").await;

        let event = store
            .create_event(
                account_id,
                "coincident-event",
                "Coincident Event",
                "2099-07-12 10:00",
            )
            .await
            .unwrap();
        store
            .update_event(
                account_id,
                event.id,
                &EventFields {
                    slug: event.slug,
                    title: event.title,
                    tagline: String::new(),
                    starts_at: event.starts_at,
                    ends_at: event.ends_at,
                    timezone: event.timezone,
                    status: "published".into(),
                    summary: "Coincident event summary".into(),
                    area_name: "Public area".into(),
                    address: "Event Secret Address".into(),
                    entry_instructions: String::new(),
                    private_details: String::new(),
                    notice_html: String::new(),
                    quick_plan_html: String::new(),
                },
            )
            .await
            .unwrap();
        set_public(&store, account_id, "event", event.id, "summary").await;
        let coincident_entry =
            entry(&store, account_id, "Coincident Entry", "2099-07-12 10:00").await;
        set_public(
            &store,
            account_id,
            "calendar_entry",
            coincident_entry,
            "summary",
        )
        .await;

        let circle_cookie = guest_cookie(&store, account_id, circle_person.id).await;
        let excluded_cookie = guest_cookie(&store, account_id, excluded_person.id).await;
        let owner = seed_session(&store, owner_identity_id, account_id).await;
        let app = site(&store, account_id).await;

        let (status, _, anonymous) = get(&app, "/calendar?month=2099-07").await;
        assert_eq!(status, StatusCode::OK);
        let anonymous = text(anonymous);
        assert!(anonymous.contains(">Busy<"));
        assert!(!anonymous.contains("BUSY SECRET"));
        assert!(!anonymous.contains("BUSY SECRET Location"));
        assert!(anonymous.contains("Excluded Public"));
        assert!(!anonymous.contains("Circle Summary"));
        assert!(!anonymous.contains("Owner Secret"));
        assert_eq!(anonymous.matches("Coincident Event").count(), 2);
        assert_eq!(anonymous.matches("Coincident Entry").count(), 2);

        let (_, _, circle) = get_with_cookie(&app, "/calendar?month=2099-07", &circle_cookie).await;
        let circle = text(circle);
        assert!(circle.contains("Circle Summary"));
        assert!(circle.contains("Excluded Public"));
        assert!(!circle.contains("Owner Secret"));

        let (_, _, excluded) =
            get_with_cookie(&app, "/calendar?month=2099-07", &excluded_cookie).await;
        let excluded = text(excluded);
        assert!(!excluded.contains("Excluded Public"));
        assert!(!excluded.contains("Circle Summary"));
        assert!(excluded.contains("Coincident Event"));

        let (_, _, owner_body) =
            get_with_cookie(&app, "/calendar?month=2099-07", &owner.cookie).await;
        let owner_body = text(owner_body);
        for secret in [
            "BUSY SECRET",
            "BUSY SECRET Location",
            "Circle Summary",
            "Excluded Public",
            "Owner Secret",
        ] {
            assert!(
                owner_body.contains(secret),
                "owner calendar omitted {secret}"
            );
        }
    }

    #[tokio::test]
    async fn calendar_guest_and_feed_are_isolated_from_other_accounts() {
        let store = Store::connect_in_memory().await;
        let (_, account_a) = store
            .signup_with_password("Owner A", "calendar-a@example.com", "hash")
            .await
            .unwrap();
        let (_, account_b) = store
            .signup_with_password("Owner B", "calendar-b@example.com", "hash")
            .await
            .unwrap();
        let person_a = store.create_person(account_a, "Guest A", "").await.unwrap();
        let a_id = entry(&store, account_a, "ACCOUNT A ONLY", "2099-08-10 10:00").await;
        let b_id = entry(&store, account_b, "ACCOUNT B SECRET", "2099-08-10 10:00").await;
        set_public(&store, account_a, "calendar_entry", a_id, "summary").await;
        set_public(&store, account_b, "calendar_entry", b_id, "summary").await;

        let cookie = guest_cookie(&store, account_a, person_a.id).await;
        let raw = "account-a-feed";
        store
            .mint_calendar_feed(account_a, person_a.id, &hash_token(raw), raw)
            .await
            .unwrap();
        let app = site(&store, account_a).await;

        let (_, _, page_body) = get_with_cookie(&app, "/calendar?month=2099-08", &cookie).await;
        let page_body = text(page_body);
        assert!(page_body.contains("ACCOUNT A ONLY"));
        assert!(!page_body.contains("ACCOUNT B SECRET"));

        let (status, _, feed_body) = get(&app, &format!("/calendar/{raw}.ics")).await;
        assert_eq!(status, StatusCode::OK);
        let feed_body = text(feed_body);
        assert!(feed_body.contains("ACCOUNT A ONLY"));
        assert!(!feed_body.contains("ACCOUNT B SECRET"));
    }

    fn vevent<'a>(body: &'a str, uid: &str) -> &'a str {
        let uid = format!("UID:{uid}@ronitnath.com");
        let uid_at = body.find(&uid).unwrap_or_else(|| panic!("missing {uid}"));
        let start = body[..uid_at].rfind("BEGIN:VEVENT").unwrap();
        let end = uid_at + body[uid_at..].find("END:VEVENT").unwrap();
        &body[start..end]
    }

    #[tokio::test]
    async fn person_feed_redaction_revoke_use_race_double_mint_and_no_store() {
        let store = Store::connect_in_memory().await;
        let (_, account_id) = store
            .signup_with_password("Owner", "feed-owner@example.com", "hash")
            .await
            .unwrap();
        let person = store
            .create_person(account_id, "Feed Person", "")
            .await
            .unwrap();

        let event = store
            .create_event(
                account_id,
                "summary-feed-event",
                "Summary Event",
                "2099-07-13 10:00",
            )
            .await
            .unwrap();
        store
            .update_event(
                account_id,
                event.id,
                &EventFields {
                    slug: event.slug,
                    title: event.title,
                    tagline: String::new(),
                    starts_at: event.starts_at,
                    ends_at: Some("2099-07-13 11:00".into()),
                    timezone: event.timezone,
                    status: "published".into(),
                    summary: "SUMMARY EVENT DESCRIPTION".into(),
                    area_name: "Summary area".into(),
                    address: "SUMMARY EVENT LOCATION".into(),
                    entry_instructions: String::new(),
                    private_details: String::new(),
                    notice_html: String::new(),
                    quick_plan_html: String::new(),
                },
            )
            .await
            .unwrap();
        let event_policy = store
            .find_audience_policy(account_id, "event", event.id)
            .await
            .unwrap()
            .unwrap();
        store
            .set_person_override(
                account_id,
                event_policy.id,
                person.id,
                Some("include"),
                Some("summary"),
            )
            .await
            .unwrap();

        let busy_id = entry(&store, account_id, "BUSY ENTRY SECRET", "2099-07-14 09:00").await;
        let summary_id = entry(&store, account_id, "Summary Entry", "2099-07-14 10:00").await;
        let full_id = entry(&store, account_id, "Full Entry", "2099-07-15 10:00").await;
        for (id, level) in [
            (busy_id, "busy"),
            (summary_id, "summary"),
            (full_id, "full"),
        ] {
            let policy = store
                .find_audience_policy(account_id, "calendar_entry", id)
                .await
                .unwrap()
                .unwrap();
            store
                .set_person_override(
                    account_id,
                    policy.id,
                    person.id,
                    Some("include"),
                    Some(level),
                )
                .await
                .unwrap();
        }

        let raw = "person-feed-token";
        let first_mint = store
            .mint_calendar_feed(account_id, person.id, &hash_token(raw), raw)
            .await
            .unwrap();
        let second_mint = store
            .mint_calendar_feed(account_id, person.id, &hash_token(raw), raw)
            .await
            .unwrap();
        assert_eq!(first_mint.id, second_mint.id, "double mint must upsert");
        let app = site(&store, account_id).await;
        assert_eq!(
            get(&app, &format!("/calendar/{raw}")).await.0,
            StatusCode::NOT_FOUND
        );
        let path = format!("/calendar/{raw}.ics");
        let (status, headers, body) = get(&app, &path).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            headers[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/calendar")
        );
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
        let body = text(body);
        let busy = vevent(&body, &format!("entry-{busy_id}"));
        assert!(busy.contains("SUMMARY:Busy"));
        assert!(!busy.contains("BUSY ENTRY SECRET"));
        assert!(!busy.contains("LOCATION:"));
        assert!(!busy.contains("DESCRIPTION:"));
        let summary_event = vevent(&body, &format!("event-{}", event.id));
        assert!(summary_event.contains("SUMMARY:Summary Event"));
        assert!(!summary_event.contains("SUMMARY EVENT LOCATION"));
        assert!(!summary_event.contains("SUMMARY EVENT DESCRIPTION"));
        assert!(!summary_event.contains("LOCATION:"));
        assert!(!summary_event.contains("DESCRIPTION:"));
        let summary = vevent(&body, &format!("entry-{summary_id}"));
        assert!(summary.contains("SUMMARY:Summary Entry"));
        assert!(!summary.contains("LOCATION:"));
        assert!(!summary.contains("DESCRIPTION:"));
        let full = vevent(&body, &format!("entry-{full_id}"));
        assert!(full.contains("SUMMARY:Full Entry"));
        assert!(full.contains("LOCATION:Full Entry Location"));
        assert!(full.contains("DESCRIPTION:Full Entry Notes"));

        let resolved_before_revoke = store
            .resolve_calendar_feed(&hash_token(raw))
            .await
            .unwrap()
            .unwrap();
        store
            .revoke_calendar_feed(account_id, person.id)
            .await
            .unwrap();
        assert_eq!(
            store
                .touch_calendar_feed(resolved_before_revoke.id)
                .await
                .unwrap(),
            0,
            "a revoke between resolve and conditional touch must win"
        );
        assert_eq!(get(&app, &path).await.0, StatusCode::NOT_FOUND);
        assert_eq!(
            get(&app, "/calendar/not-a-token.ics").await.0,
            StatusCode::NOT_FOUND
        );
    }
}
