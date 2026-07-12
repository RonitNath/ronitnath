//! Human-readable date formatting, exposed to templates as askama filters.
//!
//! The store keeps two flavors of timestamp string, both naive:
//!
//! - event wall times (`starts_at`/`ends_at`): `YYYY-MM-DD HH:MM`, local to
//!   the event's timezone — never converted, just pretty-printed.
//! - system timestamps (`created_at`, `last_used_at`, audit `at`, …):
//!   sqlite's `datetime('now')`, i.e. `YYYY-MM-DD HH:MM:SS` in UTC.
//!
//! The seconds field is what tells them apart, so `human_dt` tags
//! seconds-bearing inputs with "UTC" and leaves wall times untagged.
//! Anything unparsable is returned unchanged rather than erroring — a
//! malformed date should never take down a page.

use std::fmt;

use time::PrimitiveDateTime;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

const WALL: &[BorrowedFormatItem<'_>] = format_description!("[year]-[month]-[day] [hour]:[minute]");
const SYSTEM: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
const LONG: &[BorrowedFormatItem<'_>] = format_description!(
    "[weekday repr:long], [month repr:long] [day padding:none], [year] · [hour repr:12 padding:none]:[minute] [period]"
);
const LONG_DATE: &[BorrowedFormatItem<'_>] =
    format_description!("[weekday repr:long], [month repr:long] [day padding:none], [year]");
const SHORT: &[BorrowedFormatItem<'_>] = format_description!(
    "[month repr:short] [day padding:none], [year] · [hour repr:12 padding:none]:[minute] [period]"
);

/// "2026-07-04 13:00" → "Saturday, July 4, 2026 · 1:00 PM";
/// "2026-07-03 05:12:33" → "Jul 3, 2026 · 5:12 AM UTC".
pub fn human_datetime(raw: &str) -> String {
    let raw = raw.trim();
    if let Ok(dt) = PrimitiveDateTime::parse(raw, WALL) {
        return dt.format(LONG).unwrap_or_else(|_| raw.to_string());
    }
    if let Ok(dt) = PrimitiveDateTime::parse(raw, SYSTEM) {
        return dt
            .format(SHORT)
            .map(|s| format!("{s} UTC"))
            .unwrap_or_else(|_| raw.to_string());
    }
    raw.to_string()
}

/// Date only: "2025-12-06 17:00" → "Saturday, December 6, 2025".
pub fn human_date(raw: &str) -> String {
    let raw = raw.trim();
    for fmt in [WALL, SYSTEM] {
        if let Ok(dt) = PrimitiveDateTime::parse(raw, fmt) {
            return dt.format(LONG_DATE).unwrap_or_else(|_| raw.to_string());
        }
    }
    raw.to_string()
}

/// `|human_dt` — full date and time.
#[askama::filter_fn]
pub fn human_dt(value: &dyn fmt::Display, _: &dyn askama::Values) -> askama::Result<String> {
    Ok(human_datetime(&value.to_string()))
}

/// `|human_day` — date only, for lists where the time is noise.
#[askama::filter_fn]
pub fn human_day(value: &dyn fmt::Display, _: &dyn askama::Values) -> askama::Result<String> {
    Ok(human_date(&value.to_string()))
}

/// `|human_tz` — friendly names for the IANA zones events actually use;
/// unknown zones pass through as-is.
#[askama::filter_fn]
pub fn human_tz(value: &dyn fmt::Display, _: &dyn askama::Values) -> askama::Result<String> {
    let raw = value.to_string();
    Ok(match raw.as_str() {
        "America/Los_Angeles" => "Pacific Time".to_string(),
        "America/New_York" => "Eastern Time".to_string(),
        _ => raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_event_wall_times() {
        assert_eq!(human_datetime("2026-07-04 13:00"), "Saturday, July 4, 2026 · 1:00 PM");
        assert_eq!(human_date("2025-12-06 17:00"), "Saturday, December 6, 2025");
    }

    #[test]
    fn tags_system_timestamps_as_utc() {
        assert_eq!(human_datetime("2026-07-03 05:12:33"), "Jul 3, 2026 · 5:12 AM UTC");
    }

    #[test]
    fn passes_garbage_through_unchanged() {
        assert_eq!(human_datetime("soonish"), "soonish");
        assert_eq!(human_date(""), "");
    }
}
