//! Where `?next=` is allowed to send somebody.
//!
//! An open redirect is a phishing primitive: a link that reads
//! `https://ronitnath.com/auth?next=https://ronitnath.com.evil/…` carries this
//! site's name and lands somewhere else, and the reader who checked the domain
//! before clicking checked the right thing and still lost.
//!
//! So the parameter is not sanitised, it is *validated*, and anything that
//! fails falls back to the member shell. The rule is one sentence: a single
//! absolute path on this origin. That excludes a scheme, an authority, a
//! protocol-relative `//host`, a backslash (which some browsers normalise into
//! a slash before parsing the authority), and anything carrying a control
//! character — including the newline that would otherwise let a value split
//! the `Location` header.

/// Where a visitor lands when the parameter says nothing usable.
pub const DEFAULT: &str = "/app";

/// The paths a redirect may not target however well-formed it is.
///
/// `/auth` is the only one: bouncing a fresh sign-in back to the sign-in page
/// is a loop, and the reader reads it as the sign-in having failed.
const REFUSED: &[&str] = &["/auth"];

/// A same-origin path, or [`DEFAULT`].
#[must_use]
pub fn validate(raw: Option<&str>) -> String {
    raw.and_then(|candidate| is_same_origin_path(candidate).then(|| candidate.to_owned()))
        .unwrap_or_else(|| DEFAULT.to_owned())
}

/// Whether a string is one absolute path on this origin.
#[must_use]
pub fn is_same_origin_path(candidate: &str) -> bool {
    let mut characters = candidate.chars();
    if characters.next() != Some('/') {
        return false;
    }
    // `//host` is protocol-relative and `/\host` is the same thing to a
    // browser that normalises backslashes before parsing the authority.
    if matches!(characters.next(), Some('/' | '\\')) {
        return false;
    }
    if candidate.chars().any(char::is_control) {
        return false;
    }
    let path = candidate
        .split_once(['?', '#'])
        .map_or(candidate, |(path, _)| path);
    !REFUSED
        .iter()
        .any(|refused| path == *refused || path.starts_with(&format!("{refused}/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_on_this_origin_survives_with_its_query_and_fragment() {
        for path in ["/app", "/org/documents", "/app?tab=sessions", "/app#top"] {
            assert_eq!(validate(Some(path)), path, "{path}");
        }
    }

    #[test]
    fn nothing_that_could_name_another_host_gets_through() {
        for hostile in [
            "https://evil.example/",
            "//evil.example/",
            "/\\evil.example/",
            "http:/evil.example",
            "javascript:alert(1)",
            "evil.example",
            "",
        ] {
            assert_eq!(validate(Some(hostile)), DEFAULT, "{hostile} was accepted");
        }
    }

    #[test]
    fn a_value_cannot_split_the_location_header() {
        for injection in [
            "/app\r\nSet-Cookie: rn_session=stolen",
            "/app\nLocation: https://evil.example",
            "/app\u{0}",
        ] {
            assert_eq!(validate(Some(injection)), DEFAULT);
        }
    }

    #[test]
    fn signing_in_never_lands_back_on_the_sign_in_page() {
        assert_eq!(validate(Some("/auth")), DEFAULT);
        assert_eq!(validate(Some("/auth/register")), DEFAULT);
        assert_eq!(validate(Some("/auth?next=/app")), DEFAULT);
        // A path that merely starts with the same letters is a different page.
        assert_eq!(validate(Some("/authors")), "/authors");
    }

    #[test]
    fn an_absent_parameter_is_the_member_shell() {
        assert_eq!(validate(None), DEFAULT);
    }
}
