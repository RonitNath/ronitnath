//! The public presence: server-rendered pages that need no bundle to be useful.
//!
//! `/` is the whole surface this leg owns. It is never role-routed
//! (`docs/rebuild/plan.md` §API) — a signed-in operator and an anonymous
//! visitor get the identical document, and the tier apps live at their own
//! paths. What the browser adds afterwards is the sky: the starscape bundle
//! paints a live view of tonight's sky behind the page, and a CSS starfield
//! holds that layer until it does — or forever, under reduced motion.

use askama::Template;
use askama_web::WebTemplate;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(landing))
}

/// Which of the two authored themes a document is rendered in.
///
/// Server-rendered, per the shipped rule that the theme is a reload and only
/// the toggle is client-side: the choice arrives as the `rn_theme` cookie the
/// toggle writes, or — for a first visit — as the `Sec-CH-Prefers-Color-Scheme`
/// client hint. Neither is guaranteed, so the head also carries a three-line
/// script that corrects the attribute from `matchMedia` before first paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }
}

/// Read the theme out of a request's headers. The cookie is an explicit choice
/// and outranks the hint, which is only what the operating system prefers.
#[must_use]
pub fn theme_for(headers: &HeaderMap) -> Theme {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let from_cookie = header("cookie")
        .and_then(|cookies| {
            cookies
                .split(';')
                .filter_map(|pair| pair.split_once('='))
                .find(|(name, _)| name.trim() == "rn_theme")
                .map(|(_, value)| value)
        })
        .and_then(Theme::parse);
    from_cookie
        .or_else(|| header("sec-ch-prefers-color-scheme").and_then(Theme::parse))
        .unwrap_or(Theme::Dark)
}

#[derive(Template, WebTemplate)]
#[template(path = "landing.html")]
struct Landing {
    theme: &'static str,
    version: String,
}

async fn landing(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let page = Landing {
        theme: theme_for(&headers).as_str(),
        version: state.version.to_string(),
    };
    (
        [
            // Ask for the hint so the *next* navigation is server-rendered in
            // the right theme, and tell caches the document depends on it.
            ("accept-ch", "Sec-CH-Prefers-Color-Scheme"),
            ("vary", "Sec-CH-Prefers-Color-Scheme, Cookie"),
        ],
        page,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(*name, HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    #[test]
    fn a_first_visit_with_no_signal_renders_the_night_theme() {
        assert_eq!(theme_for(&headers(&[])), Theme::Dark);
    }

    #[test]
    fn the_client_hint_chooses_the_theme_when_nothing_was_picked() {
        let hint = headers(&[("sec-ch-prefers-color-scheme", "light")]);
        assert_eq!(theme_for(&hint), Theme::Light);
    }

    #[test]
    fn an_explicit_choice_outranks_what_the_operating_system_prefers() {
        let both = headers(&[
            ("sec-ch-prefers-color-scheme", "light"),
            ("cookie", "other=1; rn_theme=dark"),
        ]);
        assert_eq!(theme_for(&both), Theme::Dark);
    }

    #[test]
    fn an_unrecognised_value_falls_through_instead_of_rendering_nothing() {
        let junk = headers(&[("cookie", "rn_theme=chartreuse")]);
        assert_eq!(theme_for(&junk), Theme::Dark);
        let junk_with_hint = headers(&[
            ("cookie", "rn_theme=chartreuse"),
            ("sec-ch-prefers-color-scheme", "light"),
        ]);
        assert_eq!(theme_for(&junk_with_hint), Theme::Light);
    }
}
